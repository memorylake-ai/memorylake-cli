// JSON helper for the macOS quick actions. macOS has no jq or (reliably) python3,
// but osascript's JavaScript runtime is always there.
//
//   memorylake … | osascript -l JavaScript json.js get items.0.id
//   memorylake … | osascript -l JavaScript json.js lines items name
//   memorylake … | osascript -l JavaScript json.js find items name "Alpha" id
//   memorylake … | osascript -l JavaScript json.js search zh|en
//   memorylake … | osascript -l JavaScript json.js import zh|en
ObjC.import('Foundation');

function readStdin() {
  var data = $.NSFileHandle.fileHandleWithStandardInput.readDataToEndOfFile;
  return ObjC.unwrap($.NSString.alloc.initWithDataEncoding(data, $.NSUTF8StringEncoding)) || '';
}

function dig(obj, path) {
  var parts = path === '' ? [] : path.split('.');
  for (var i = 0; i < parts.length; i++) {
    if (obj === null || obj === undefined) return undefined;
    obj = obj[parts[i]];
  }
  return obj;
}

function str(v) { return v === null || v === undefined ? '' : String(v); }

var STRINGS = {
  zh: { facts: '【记忆】', docs: '【文档】', none: '没有找到相关记忆。',
        indexed: ' 个已提交索引', dup: ' 个已存在', failed: ' 个导入失败' },
  en: { facts: '[Memories]', docs: '[Documents]', none: 'No related memories found.',
        indexed: ' queued for indexing', dup: ' already present', failed: ' failed to import' }
};

function formatSearch(d, lang) {
  var s = STRINGS[lang] || STRINGS.en;
  var out = [];
  var facts = d.facts || [], docs = d.documents || [];
  if (facts.length) {
    out.push(s.facts);
    facts.forEach(function (f, i) { out.push((i + 1) + '. ' + str(f.fact).trim()); });
  }
  if (docs.length) {
    out.push((out.length ? '\n' : '') + s.docs);
    docs.forEach(function (x, i) {
      var title = x.document_name || x.file_name || x.document_id || '';
      var snips = (x.items || []).map(function (it) { return it.text; }).filter(Boolean);
      var snip = str(snips.length ? snips[0] : x.document_summary).trim().replace(/\s+/g, ' ');
      out.push((i + 1) + '. ' + title + (snip ? '\n   ' + snip.slice(0, 200) : ''));
    });
  }
  return out.length ? out.join('\n') : s.none;
}

function formatImport(d, lang) {
  var s = STRINGS[lang] || STRINGS.en;
  var parts = [str(d.success_count || 0) + s.indexed];
  if (d.duplicate_count) parts.push(d.duplicate_count + s.dup);
  if (d.failure_count) parts.push(d.failure_count + s.failed);
  return parts.join(lang === 'zh' ? '，' : ', ');
}

function run(argv) {
  var raw = readStdin();
  if (raw.trim() === "") return "";
  var d = JSON.parse(raw);
  var mode = argv[0];
  if (mode === 'get') return str(dig(d, argv[1]));
  if (mode === 'lines') {
    return (dig(d, argv[1]) || []).map(function (x) { return str(x[argv[2]]); }).join('\n');
  }
  if (mode === 'find') {
    var arr = dig(d, argv[1]) || [];
    for (var i = 0; i < arr.length; i++) if (str(arr[i][argv[2]]) === argv[3]) return str(arr[i][argv[4]]);
    return '';
  }
  if (mode === 'search') return formatSearch(d, argv[1]);
  if (mode === 'import') return formatImport(d, argv[1]);
  throw new Error('unknown mode ' + mode);
}
