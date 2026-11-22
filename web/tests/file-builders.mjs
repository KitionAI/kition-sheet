// Test inputs built here byte by byte (the repository holds no files other applications wrote):
// a stored (uncompressed) zip, a compound file (OLE2) with one stream, and a small Excel 97-2003
// (BIFF8) workbook. Used by tests/gate-file.mjs.

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n
  for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
  return c >>> 0
})

function crc32(bytes) {
  let c = 0xffffffff
  for (const b of bytes) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8)
  return (c ^ 0xffffffff) >>> 0
}

/** A zip of `files` (name → string or bytes), stored without compression. */
export function zip(files) {
  const enc = new TextEncoder()
  const locals = []
  const centrals = []
  let offset = 0
  for (const [name, content] of Object.entries(files)) {
    const data = typeof content === 'string' ? enc.encode(content) : content
    const nameBytes = enc.encode(name)
    const crc = crc32(data)
    const local = new DataView(new ArrayBuffer(30))
    local.setUint32(0, 0x04034b50, true)
    local.setUint16(4, 20, true)
    local.setUint32(14, crc, true)
    local.setUint32(18, data.length, true)
    local.setUint32(22, data.length, true)
    local.setUint16(26, nameBytes.length, true)
    locals.push(new Uint8Array(local.buffer), nameBytes, data)
    const central = new DataView(new ArrayBuffer(46))
    central.setUint32(0, 0x02014b50, true)
    central.setUint16(4, 20, true)
    central.setUint16(6, 20, true)
    central.setUint32(16, crc, true)
    central.setUint32(20, data.length, true)
    central.setUint32(24, data.length, true)
    central.setUint16(28, nameBytes.length, true)
    central.setUint32(42, offset, true)
    centrals.push(new Uint8Array(central.buffer), nameBytes)
    offset += 30 + nameBytes.length + data.length
  }
  const centralSize = centrals.reduce((n, part) => n + part.length, 0)
  const end = new DataView(new ArrayBuffer(22))
  end.setUint32(0, 0x06054b50, true)
  end.setUint16(8, Object.keys(files).length, true)
  end.setUint16(10, Object.keys(files).length, true)
  end.setUint32(12, centralSize, true)
  end.setUint32(16, offset, true)
  return concat([...locals, ...centrals, new Uint8Array(end.buffer)])
}

export function concat(parts) {
  const out = new Uint8Array(parts.reduce((n, part) => n + part.length, 0))
  let at = 0
  for (const part of parts) {
    out.set(part, at)
    at += part.length
  }
  return out
}

/** A compound file (OLE2, 512-byte sectors) holding `streams` (name → bytes), each in regular sectors. */
export function compound(streams) {
  const entries = Object.entries(streams).map(([name, data]) => {
    const padded = new Uint8Array(Math.ceil(Math.max(data.length, 4096) / 512) * 512)
    padded.set(data)
    return { name, data: padded, size: data.length < 4096 ? 4096 : data.length }
  })
  const sectors = entries.reduce((n, e) => n + e.data.length / 512, 0)
  if (sectors + 2 > 128) throw new Error('too large for one FAT sector')
  const out = new Uint8Array(512 * 3 + sectors * 512)
  const view = new DataView(out.buffer)
  out.set([0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1])
  view.setUint16(24, 0x3e, true)
  view.setUint16(26, 3, true)
  view.setUint16(28, 0xfffe, true)
  view.setUint16(30, 9, true)
  view.setUint16(32, 6, true)
  view.setUint32(44, 1, true)
  view.setUint32(48, 1, true)
  view.setUint32(56, 4096, true)
  view.setUint32(60, 0xfffffffe, true)
  view.setUint32(68, 0xfffffffe, true)
  for (let i = 0; i < 109; i += 1) view.setUint32(76 + i * 4, 0xffffffff, true)
  view.setUint32(76, 0, true)
  const fat = 512
  for (let i = 0; i < 128; i += 1) view.setUint32(fat + i * 4, 0xffffffff, true)
  view.setUint32(fat, 0xfffffffd, true)
  view.setUint32(fat + 4, 0xfffffffe, true)
  let next = 2
  const dir = 1024
  const writeEntry = (k, name, kind, start, size) => {
    const e = dir + k * 128
    const units = [...name].map((c) => c.charCodeAt(0))
    units.forEach((u, i) => view.setUint16(e + i * 2, u, true))
    view.setUint16(e + 64, (units.length + 1) * 2, true)
    out[e + 66] = kind
    for (const at of [68, 72, 76]) view.setUint32(e + at, 0xffffffff, true)
    if (k === 0) view.setUint32(e + 76, 1, true)
    view.setUint32(e + 116, start, true)
    view.setUint32(e + 120, size, true)
  }
  writeEntry(0, 'Root Entry', 5, 0xfffffffe, 0)
  entries.forEach((entry, i) => {
    const count = entry.data.length / 512
    for (let s = 0; s < count; s += 1) view.setUint32(fat + (next + s) * 4, s + 1 === count ? 0xfffffffe : next + s + 1, true)
    out.set(entry.data, 512 * (next + 1))
    writeEntry(i + 1, entry.name, 2, next, entry.size)
    next += count
  })
  return out
}

/** One BIFF record. */
function rec(kind, data) {
  const head = new DataView(new ArrayBuffer(4))
  head.setUint16(0, kind, true)
  head.setUint16(2, data.length, true)
  return concat([new Uint8Array(head.buffer), data])
}

const u16 = (v) => new Uint8Array([v & 0xff, (v >> 8) & 0xff])
const u32 = (v) => new Uint8Array([v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, (v >>> 24) & 0xff])
const f64 = (v) => {
  const b = new DataView(new ArrayBuffer(8))
  b.setFloat64(0, v, true)
  return new Uint8Array(b.buffer)
}
/** UTF-16 characters of a string. */
const wide = (text) => concat([...text].map((c) => u16(c.charCodeAt(0))))
const shortString = (text) => concat([new Uint8Array([text.length, 1]), wide(text)])
const bytes = (...list) => new Uint8Array(list)

/**
 * A small Excel 97-2003 workbook: a sheet named `sheet` with a bold, filled header row (`headers`),
 * the `rows` below (numbers or strings), a SUM of column B under them, A1:B1 merged, column A
 * `widthChars` wide, frozen below row 1.
 */
export function biffWorkbook({ sheet = '数据', headers = ['名称', '数量'], rows = [['苹果', 3], ['香蕉', 5]], widthChars = 20 } = {}) {
  const strings = [...new Set([...headers, ...rows.flatMap((row) => row.filter((v) => typeof v === 'string'))])]
  const bof = (dt) => rec(0x0809, concat([u16(0x0600), u16(dt), new Uint8Array(12)]))
  const font = (twips, bold, name) => rec(0x0031, concat([u16(twips), u16(0), u16(0x7fff), u16(bold ? 700 : 400), u16(0), bytes(0, 0, 134, 0), shortString(name)]))
  const xf = (fontIndex, fill, cell) =>
    rec(0x00e0, concat([u16(fontIndex), u16(0), u16(cell ? 1 : 0xfff5), bytes(0x22, 0, 0, 0), u16(0x1111), u16(8 | (8 << 7)), u32(8 | (8 << 7) | ((fill ? 1 : 0) << 26)), u16((fill || 0) | (0x41 << 7))]))
  const globalsHead = [bof(5), font(220, false, '宋体'), font(220, true, '宋体'), font(220, false, '宋体'), font(220, false, '宋体')]
  for (let i = 0; i < 15; i += 1) globalsHead.push(xf(0, 0, false))
  globalsHead.push(xf(0, 0, true), xf(1, 13, true))
  const boundsheetAt = globalsHead.reduce((n, r) => n + r.length, 0)
  const boundsheet = rec(0x0085, concat([u32(0), bytes(0, 0), shortString(sheet)]))
  const sst = rec(0x00fc, concat([u32(strings.length), u32(strings.length), ...strings.map((s) => concat([u16(s.length), bytes(1), wide(s)]))]))
  const globals = concat([...globalsHead, boundsheet, sst, rec(0x000a, new Uint8Array())])
  const cell = (kind, row, col, xfIndex, rest) => rec(kind, concat([u16(row), u16(col), u16(xfIndex), rest]))
  const value = (row, col, v, xfIndex = 15) => (typeof v === 'number' ? cell(0x0203, row, col, xfIndex, f64(v)) : cell(0x00fd, row, col, xfIndex, u32(strings.indexOf(v))))
  const body = [bof(0x10), rec(0x007d, concat([u16(0), u16(0), u16(widthChars * 256), u16(15), u16(0), u16(0)]))]
  headers.forEach((h, c) => body.push(value(0, c, h, 16)))
  rows.forEach((row, r) => row.forEach((v, c) => body.push(value(r + 1, c, v))))
  const total = rows.reduce((n, row) => n + (typeof row[1] === 'number' ? row[1] : 0), 0)
  const last = rows.length + 1
  // =SUM(B2:B<last>) with its result.
  body.push(cell(0x0006, last, 1, 15, concat([f64(total), new Uint8Array(6), u16(13), bytes(0x25), u16(1), u16(last - 1), u16(0xc001), u16(0xc001), bytes(0x19, 0x10, 0, 0)])))
  body.push(rec(0x00e5, concat([u16(1), u16(0), u16(0), u16(0), u16(1)])))
  body.push(rec(0x023e, concat([u16(0x060e), new Uint8Array(16)])))
  body.push(rec(0x0041, concat([u16(0), u16(1), u16(1), u16(0), u16(2)])))
  body.push(rec(0x000a, new Uint8Array()))
  const stream = concat([globals, ...body])
  // The sheet's stream position, in its BOUNDSHEET8 record.
  new DataView(stream.buffer).setUint32(boundsheetAt + 4, globals.length, true)
  return compound({ Workbook: stream })
}
