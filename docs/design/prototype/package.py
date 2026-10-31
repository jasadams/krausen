#!/usr/bin/env python3
"""Package the built preview into one offline HTML file for design review."""
import base64
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parent
output = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else root / "standalone.html"


def data_url(path, mime):
    return f"data:{mime};base64," + base64.b64encode(path.read_bytes()).decode()


html = (root / "index.html").read_text()
font_css = (root / "fonts/fonts.css").read_text()
for font in (root / "fonts").glob("*.ttf"):
    font_css = font_css.replace(f"url({font.name})", f"url({data_url(font, 'font/ttf')})")
styles = "\n".join([font_css] + [(root / name).read_text() for name in ["tokens.css", "components.css", "layout.css"]])
html = re.sub(r'\s*<link rel="stylesheet"[^>]+>', "", html)
html = html.replace("</head>", f"<style>{styles}</style>\n</head>")
mark = data_url(root / "assets/mark.svg", "image/svg+xml")
html = html.replace("assets/mark.svg", mark)
glue = (root / "pkg/calculator.js").read_text()
glue = glue.replace("export function calculate", "function calculate")
glue = glue.replace("export { initSync, __wbg_init as default };", "const init = __wbg_init;")
kit = re.sub(r"^export ", "", (root / "components.js").read_text(), flags=re.M)
style_module = re.sub(r"^export ", "", (root / "styles.js").read_text(), flags=re.M)
catalogue_module = re.sub(r"^export ", "", (root / "catalogue.js").read_text(), flags=re.M)
app = re.sub(r"^import[\s\S]*?;\s*", "", (root / "app.js").read_text(), flags=re.M)
encoded_wasm = base64.b64encode((root / "pkg/calculator_bg.wasm").read_bytes()).decode()
app = app.replace("await init();", "await init({module_or_path: Uint8Array.from(atob(EMBEDDED_ENGINE), character => character.charCodeAt(0))});")
app = app.replace("assets/mark.svg", mark)
script = f"const EMBEDDED_ENGINE = '{encoded_wasm}';\n{glue}\n{kit}\n{style_module}\n{catalogue_module}\n{app}"
script = script.replace("</script", "<\\/script")
html = html.replace('<script type="module" src="app.js"></script>', f'<script type="module">{script}</script>')
output.write_text(html)
print(f"Offline preview: {output} ({output.stat().st_size:,} bytes)")
