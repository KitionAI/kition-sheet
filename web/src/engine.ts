import type { KitModuleManifest } from '@kition/kit-editor'

/** The engine, from this package's own folder. */
export function engineManifest(): KitModuleManifest {
  return {
    kitId: 'kit.sheet',
    base: new URL('./engine/', document.baseURI).href,
    module: 'kition_sheet_kit',
    documentClass: 'SheetDocument',
    // The web build embeds no CJK face: Chinese, and the metric-compatible stand-ins for Office's
    // fonts (decision F1), so cell text takes the width it takes in Excel, copied from
    // kition-fonts by build-engine.sh.
    fonts: [
      'DroidSansFallbackFull.ttf',
      'Carlito-Regular.ttf',
      'Carlito-Bold.ttf',
      'Carlito-Italic.ttf',
      'Carlito-BoldItalic.ttf',
      'Caladea-Regular.ttf',
      'Caladea-Bold.ttf',
      'Caladea-Italic.ttf',
      'Caladea-BoldItalic.ttf',
      'Arimo-Variable.ttf',
      'Arimo-Italic-Variable.ttf',
      'Tinos-Regular.ttf',
      'Tinos-Bold.ttf',
      'Tinos-Italic.ttf',
      'Cousine-Regular.ttf',
      // 楷体's open counterpart: a Kai face (kition-fonts has no open Song or FangSong face).
      'MaShanZheng-Regular.ttf',
    ].map((file) => new URL(`./fonts/${file}`, document.baseURI).href),
  }
}
