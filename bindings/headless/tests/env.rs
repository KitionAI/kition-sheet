//! Public SDK environment sampling, traversal and identity-bound replay.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kition_env::{EnvKit, read_snapshot};
use kition_kit::{KitDocument, session::model::Exchange};
use kition_sheet_kit::SheetDocument;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn image_creation_sampling_honors_container_sample_false() {
    use kition_kit::session::model::ReplicaPort;
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("name.define", json!({"name":"Rate","refersTo":"0.2"})).unwrap();
    assert_eq!(doc.model_schema().types["Sheet"].fields["images"].sample, Some(false));
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("Book.xlsx"), doc.save(None).unwrap()).unwrap();
    let refused = Exchange::new()
        .batch(&mut doc, &json!({"ops":[{"op":"create","type":"Image","in":"sheet:1.images","at":0,"fields":{}}]}), false)
        .unwrap_err();
    assert!(refused.message.contains("content bytes"), "{refused:?}");
    let definition = Box::leak(Box::new(kition_kits::KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap()));
    let kit = EnvKit::new(definition, Some(env!("CARGO_BIN_EXE_kition-sheet").into()));
    let generated = kition_env::generate(
        &kit,
        directory.path(),
        &kition_env::GenerateOptions {
            seed: 7,
            count: 64,
            attempts: 1,
            inverse_share: 0.0,
            kinds: Some(vec!["create".into()]),
            vocabulary: Some(vec!["Candidate".into()]),
        },
    )
    .await
    .unwrap();
    let refusals = generated
        .tasks
        .iter()
        .flat_map(|task| &task.resampled)
        .chain(generated.given_up.iter().flat_map(|giveup| giveup["resampled"].as_array().into_iter().flatten()))
        .filter(|attempt| attempt["reason"] == "refused")
        .count();
    assert_eq!(refusals, 0, "sample:false images must never become invalid creation candidates: {generated:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn env_snapshots_traverse_owning_maps() {
    let mut doc = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    doc.execute("cell.set", json!({"cell":"B7","input":"7"})).unwrap();
    let cell = Exchange::new().batch(&mut doc, &json!({"load":[{"target":"cell:1!B7"}]}), true).unwrap();
    assert_eq!(cell["loaded"][0]["type"], "Cell");
    let definition = Box::leak(Box::new(kition_kits::KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap()));
    let kit = EnvKit::new(definition, Some(env!("CARGO_BIN_EXE_kition-sheet").into()));
    let snapshot = read_snapshot(&kit, "Book.xlsx", &doc.save(None).unwrap()).await.unwrap();
    assert!(snapshot.ids().contains(&"sheet:1".to_owned()));
    assert!(snapshot.ids().contains(&"cell:1!B7".to_owned()), "{:?}", snapshot.ids());
}

#[tokio::test(flavor = "multi_thread")]
async fn environment_replay_binds_created_identities() {
    let mut first = SheetDocument::create("Book.xlsx", 0, 0).unwrap();
    let bytes = first.save(None).unwrap();
    let definition = Box::leak(Box::new(kition_kits::KitDefinition::from_manifest_text(include_str!("../../../kit.json")).unwrap()));
    let kit = EnvKit::new(definition, Some(env!("CARGO_BIN_EXE_kition-sheet").into()));
    let before = read_snapshot(&kit, "Book.xlsx", &bytes).await.unwrap();
    let receipt = Exchange::new()
        .batch(&mut first, &json!({"ops":[{"op":"create","type":"Name","in":"root.names","at":0,"fields":{"name":"Rate","formula":"0.2"}}]}), false)
        .unwrap();
    let original = receipt["results"][0].clone();
    let loaded = Exchange::new().batch(&mut first, &json!({"load":[{"target":original}]}), true).unwrap();
    let changes = json!({"created":[{"id":original,"type":"Name","at":{"parent":"workbook","field":"names","index":0},"values":loaded["loaded"][0]["fields"]}],"changed":[],"moved":[],"deleted":[]});
    let setup = kition_env::ops_for(&changes, &before).unwrap();
    let edited = read_snapshot(&kit, "Book.xlsx", &first.save(None).unwrap()).await.unwrap();
    let inverse = kition_env::ops_for(&json!({"created":[],"changed":[],"moved":[],"deleted":[{"id":original,"type":"Name"}]}), &edited).unwrap();
    let reference = setup.iter().find_map(|op| op["ref"].as_str()).unwrap().to_owned();
    let task = kition_env::Task {
        id: "identity-replay".into(),
        kit: "kit.sheet".into(),
        file: "Book.xlsx".into(),
        seed: 7,
        index: 0,
        kind: "create".into(),
        instruction: "Remove Rate".into(),
        ground_truth: json!({"created":[],"changed":[],"moved":[],"deleted":[{"id":original,"type":"Name"}]}),
        inverse: true,
        setup: Some(setup),
        solution: inverse.clone(),
        bindings: [(original.as_str().unwrap().to_owned(), reference)].into_iter().collect(),
        resampled: vec![],
    };
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("Book.xlsx"), bytes).unwrap();
    let mut env = kition_env::Environment::new(kit, directory.path());
    let observation = env.reset(&task).await.unwrap();
    assert_ne!(observation.id_map[original.as_str().unwrap()], original.as_str().unwrap());
    let step = env.step("sheet_batch", json!({"path":"Book.xlsx","ops":inverse})).await.unwrap();
    assert!(step.result.get("error").is_none(), "{step:?}");
    assert_eq!(step.arguments["ops"][0]["target"], observation.id_map[original.as_str().unwrap()]);
    let (replay, _) = kition_sheet_engine::io::open_bytes("Book.xlsx", &std::fs::read(observation.workspace.join("Book.xlsx")).unwrap()).unwrap();
    assert!(replay.names.is_empty());
    env.close().await;
}
