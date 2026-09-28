from pathlib import Path
import re

root = Path(r"D:\SuperExplorer\crates\explorer-i18n\locales")
updated = 0
for path in sorted(root.glob("*/menus.ftl")):
    text = path.read_text(encoding="utf-8-sig")
    new, count = re.subn(
        r"^(menu-bookmark-toolbar =)\s*▸",
        r"\1 ⌄",
        text,
        count=1,
        flags=re.M,
    )
    if count != 1:
        raise SystemExit(f"{path.parent.name}: replacements={count}")
    if new != text:
        path.write_text(new, encoding="utf-8", newline="\n")
        updated += 1
        print(path.parent.name)
print("updated", updated)
