#!/usr/bin/env python3
"""Load the copy Nathaniel e-mailed on 2026-09-18 into a database, word for word.

  set -a; . ~/.config/thepenmarket.env; set +a; python3 scripts/load-his-copy.py [--dry-run]

Reads $BACKUP_DIR/client-copy/ (never committed: the repo is public and this text is not published
yet) and $DATABASE_URL. Safe to run twice and safe after he has edited anything:

- Pen Grading, Defining Nibs, Buyer's Guide: the body is filled only while it is still empty
  (2026-09-18-load-his-pages.sql guards every UPDATE), then a page that has his text and is still a
  draft is published. He asked for these pages on the site; the words are his. An `admin.page_his_copy`
  row in event_log then makes `import` (and so deploy.sh) leave the database alone.
- His nib glossary: each "Term: definition" line fills that glossary entry's definition only while it
  is empty, or adds the entry if the glossary has no such word. Entries stay drafts: nothing in the
  glossary is visible until he publishes it at /admin/glossary/.
"""
import os
import re
import secrets
import subprocess
import sys

DRY = "--dry-run" in sys.argv
backup = os.environ.get("BACKUP_DIR", "")
db = os.environ.get("DATABASE_URL", "")
if not backup or not db:
    sys.exit("BACKUP_DIR and DATABASE_URL must be set (source ~/.config/thepenmarket.env)")
copy = os.path.join(backup, "client-copy")
PAGES_SQL = os.path.join(copy, "2026-09-18-load-his-pages.sql")
NIBS = os.path.join(copy, "2026-09-18-nib-glossary-HELD-for-glossary-session.txt")
PAGE_SLUGS = ("pen-grading", "defining-nibs", "buyers-guide")
NIB_TOPIC = "Nibs and how they write"


def psql(sql, *, read=False):
    if DRY and not read:
        print("-- dry run, not executed:\n" + sql[:600] + ("..." if len(sql) > 600 else ""))
        return ""
    out = subprocess.run(["psql", db, "-v", "ON_ERROR_STOP=1", "-Atq", "-c" if read else "-f", sql if read else "-"],
                         input=None if read else sql, capture_output=True, text=True)
    if out.returncode != 0:
        sys.exit(out.stderr.strip())
    return out.stdout


def lit(s):
    """A dollar-quoted literal whose tag cannot occur in the text."""
    while True:
        tag = "t" + secrets.token_hex(4)
        if f"${tag}$" not in s:
            return f"${tag}${s}${tag}$"


def slugify(s):
    s = s.lower().replace("’", "").replace("'", "")
    return re.sub(r"[^a-z0-9]+", "-", s).strip("-")


def parse_nibs(text):
    """'Architect nib/Architect grind: …' -> ('Architect nib', ['Architect grind'], '…')."""
    out = []
    for line in text.splitlines():
        line = line.strip()
        m = re.match(r"^([A-Z][^:]{1,60}):\s+(.+)$", line)
        if not m:
            if out and line and line.upper() != line:
                out[-1][2] += "\n\n" + line  # a second paragraph of the entry above
            continue
        names = [n.strip() for n in re.split(r"\s*/\s*|\s+or\s+", m.group(1)) if n.strip()]
        out.append([names[0], names[1:], m.group(2).strip()])
    return out


# ---- pages ------------------------------------------------------------------------------------------
psql(open(PAGES_SQL, encoding="utf-8").read())
slugs = ",".join(f"'{s}'" for s in PAGE_SLUGS)
psql(f"UPDATE page SET status = 'published', published_at = now(), modified_at = now() "
     f"WHERE custom AND slug IN ({slugs}) AND status = 'draft' AND body_html <> '';")
print(psql(f"SELECT slug || ': ' || status || ', ' || length(body_html) || ' characters' FROM page WHERE slug IN ({slugs}) ORDER BY slug", read=True).strip())
# Record it the way an edit in /admin/ is recorded, so `import` (run by deploy.sh) refuses to wipe these
# pages: the page table is truncated on a re-import and they are not in the WordPress backup.
psql(f"INSERT INTO event_log (kind, detail) SELECT 'admin.page_his_copy', jsonb_build_object('slugs', {lit(','.join(PAGE_SLUGS))}) "
     f"WHERE EXISTS (SELECT 1 FROM page WHERE custom AND slug IN ({slugs}) AND body_html <> '') "
     f"AND NOT EXISTS (SELECT 1 FROM event_log WHERE kind = 'admin.page_his_copy');")

# ---- nib glossary -----------------------------------------------------------------------------------
entries = parse_nibs(open(NIBS, encoding="utf-8").read())
filled = added = kept = 0
for name, aliases, definition in entries:
    keys = [name] + aliases
    where = " OR ".join(f"lower(name) = lower({lit(k)}) OR slug = {lit(slugify(k))} OR {lit(k.lower())} = ANY(string_to_array(lower(aliases), ', '))" for k in keys)
    row = psql(f"SELECT id || '|' || length(definition) FROM glossary_term WHERE {where} ORDER BY id LIMIT 1", read=True).strip()
    if row:
        tid, dlen = row.split("|")
        if int(dlen) > 0:
            kept += 1
            continue
        psql(f"UPDATE glossary_term SET definition = {lit(definition)}, updated_at = now() WHERE id = {int(tid)} AND definition = '';")
        filled += 1
    else:
        psql(f"INSERT INTO glossary_term (slug, name, kind, topic, aliases, definition, status, origin) VALUES "
             f"({lit(slugify(name))}, {lit(name)}, 'term', {lit(NIB_TOPIC)}, {lit(', '.join(aliases))}, {lit(definition)}, 'draft', 'admin') ON CONFLICT (slug) DO NOTHING;")
        added += 1
print(f"nib glossary: {len(entries)} entries in his file; {filled} drafts filled, {added} drafts added, {kept} already had a definition and were left alone. None published.")
