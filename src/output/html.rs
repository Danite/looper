use crate::types::ScanManifest;

/// Format a ScanManifest as a self-contained HTML report.
pub fn format_html(manifest: &ScanManifest) -> Result<String, serde_json::Error> {
    let data_json = serde_json::to_string(manifest)?;
    let safe_json = data_json.replace("</", "<\\/");
    Ok(HTML_TEMPLATE.replace("/*__LOOPER_DATA__*/", &safe_json))
}

const HTML_TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>looper — Shadow Dependencies Report</title>
<style>
  :root { --bg: #0d1117; --fg: #c9d1d9; --border: #30363d; --accent: #58a6ff; --green: #3fb950; --yellow: #d29922; --red: #f85149; --muted: #8b949e; --surface: #161b22; }
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif; background: var(--bg); color: var(--fg); padding: 2rem; line-height: 1.5; }
  h1 { font-size: 1.5rem; margin-bottom: 0.25rem; }
  .meta { color: var(--muted); font-size: 0.875rem; margin-bottom: 1.5rem; }
  .stats { display: flex; gap: 1rem; flex-wrap: wrap; margin-bottom: 1.5rem; }
  .stat { background: var(--surface); border: 1px solid var(--border); border-radius: 6px; padding: 0.75rem 1rem; min-width: 120px; }
  .stat-value { font-size: 1.5rem; font-weight: 600; }
  .stat-label { font-size: 0.75rem; color: var(--muted); text-transform: uppercase; }
  .filters { display: flex; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 1rem; }
  .filters select, .filters input { background: var(--surface); color: var(--fg); border: 1px solid var(--border); border-radius: 4px; padding: 0.4rem 0.6rem; font-size: 0.875rem; }
  table { width: 100%; border-collapse: collapse; font-size: 0.875rem; }
  th { text-align: left; padding: 0.6rem 0.75rem; border-bottom: 2px solid var(--border); color: var(--muted); font-weight: 600; cursor: pointer; user-select: none; }
  th:hover { color: var(--accent); }
  td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border); }
  tr:hover td { background: var(--surface); }
  .conf-high { color: var(--green); }
  .conf-medium { color: var(--yellow); }
  .conf-low { color: var(--red); }
  .cat-badge { display: inline-block; padding: 0.15rem 0.5rem; border-radius: 12px; font-size: 0.75rem; font-weight: 500; background: var(--surface); border: 1px solid var(--border); }
  .clickable { cursor: pointer; }
  .clickable:hover { color: var(--accent); text-decoration: underline; }
  .toast { position: fixed; bottom: 1rem; right: 1rem; background: var(--surface); border: 1px solid var(--border); padding: 0.5rem 1rem; border-radius: 6px; font-size: 0.875rem; opacity: 0; transition: opacity 0.3s; }
  .toast.show { opacity: 1; }
</style>
</head>
<body>
<h1>looper — Shadow Dependencies Report</h1>
<div class="meta" id="meta"></div>
<div class="stats" id="stats"></div>
<div class="filters">
  <select id="filterCat"><option value="">All categories</option></select>
  <select id="filterConf"><option value="">All confidence</option><option value="high">High</option><option value="medium">Medium</option><option value="low">Low</option></select>
  <input type="text" id="filterSearch" placeholder="Search...">
</div>
<table>
  <thead><tr><th data-col="name">Name</th><th data-col="category">Category</th><th data-col="confidence">Confidence</th><th data-col="file">File</th><th data-col="line">Line</th></tr></thead>
  <tbody id="tbody"></tbody>
</table>
<div class="toast" id="toast">Copied!</div>
<script>
const DATA = /*__LOOPER_DATA__*/{};
const deps = DATA.deps || [];
const meta = document.getElementById('meta');
meta.textContent = `v${DATA.tool_version} — ${DATA.files_scanned} files scanned in ${DATA.scan_duration_ms}ms — ${DATA.scanned_at}`;

const cats = {};
const confs = { high: 0, medium: 0, low: 0 };
deps.forEach(d => { cats[d.category] = (cats[d.category]||0)+1; confs[d.confidence]++; });

const stats = document.getElementById('stats');
stats.innerHTML = `<div class="stat"><div class="stat-value">${deps.length}</div><div class="stat-label">Total</div></div>` +
  Object.entries(cats).map(([k,v]) => `<div class="stat"><div class="stat-value">${v}</div><div class="stat-label">${k}</div></div>`).join('');

const filterCat = document.getElementById('filterCat');
Object.keys(cats).sort().forEach(c => { const o = document.createElement('option'); o.value = c; o.textContent = c; filterCat.appendChild(o); });

let sortCol = 'name', sortDir = 1;
function render() {
  const fc = filterCat.value, fv = document.getElementById('filterConf').value, fs = document.getElementById('filterSearch').value.toLowerCase();
  let rows = deps.filter(d => (!fc || d.category===fc) && (!fv || d.confidence===fv) && (!fs || d.name.toLowerCase().includes(fs) || (d.locations[0]||{}).file?.toLowerCase().includes(fs)));
  rows.sort((a,b) => {
    let av, bv;
    if (sortCol==='file') { av=(a.locations[0]||{}).file||''; bv=(b.locations[0]||{}).file||''; }
    else if (sortCol==='line') { av=(a.locations[0]||{}).line||0; bv=(b.locations[0]||{}).line||0; return (av-bv)*sortDir; }
    else { av=a[sortCol]||''; bv=b[sortCol]||''; }
    return av.localeCompare(bv)*sortDir;
  });
  const tbody = document.getElementById('tbody');
  tbody.innerHTML = rows.map(d => {
    const loc = d.locations[0]||{};
    return `<tr>
      <td class="clickable" onclick="copy('${d.name.replace(/'/g,"\\'")}')">${esc(d.name)}</td>
      <td><span class="cat-badge">${d.category}</span></td>
      <td class="conf-${d.confidence}">${d.confidence}</td>
      <td class="clickable" onclick="copy('${(loc.file||'').replace(/'/g,"\\'")}')">${esc(loc.file||'')}</td>
      <td>${loc.line||''}</td>
    </tr>`;
  }).join('');
}
function esc(s) { const d=document.createElement('div'); d.textContent=s; return d.innerHTML; }
function copy(t) { navigator.clipboard.writeText(t); const toast=document.getElementById('toast'); toast.classList.add('show'); setTimeout(()=>toast.classList.remove('show'),1500); }
document.querySelectorAll('th').forEach(th => th.addEventListener('click', () => { const c=th.dataset.col; if(sortCol===c) sortDir*=-1; else { sortCol=c; sortDir=1; } render(); }));
filterCat.addEventListener('change', render);
document.getElementById('filterConf').addEventListener('change', render);
document.getElementById('filterSearch').addEventListener('input', render);
render();
</script>
</body>
</html>"##;
