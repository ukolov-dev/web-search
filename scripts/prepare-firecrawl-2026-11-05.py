#!/usr/bin/env python3
"""Freeze twelve historical source pages and prior source judgments before reader A/B."""
import hashlib
import json
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / "runs/firecrawl-2026-11-05"


def load(path):
    return json.loads((ROOT / path).read_text())


def save(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")


def digest(data):
    return hashlib.sha256(data).hexdigest()


class Visible(HTMLParser):
    def __init__(self):
        super().__init__()
        self.hidden, self.parts = 0, []

    def handle_starttag(self, tag, attrs):
        if tag in {"script", "style", "head", "template"}:
            self.hidden += 1

    def handle_endtag(self, tag):
        if tag in {"script", "style", "head", "template"} and self.hidden:
            self.hidden -= 1

    def handle_data(self, text):
        if not self.hidden:
            self.parts.append(text)


def plain(html):
    parser = Visible()
    parser.feed(html)
    return " ".join(" ".join(parser.parts).split())


assert plain("<script>foreign</script><p>one <b>two</b></p>") == "one two"
pages, gold, groups = [], [], []


def append(group, url, html, title, grade, reason, witnesses, origin, expected_date=None):
    pid = f"P{len(pages)+1:02d}"
    identifier = f"C{sum(p['group'] == group for p in pages)+1:03d}"
    assert witnesses and all(needle in plain(html) for needle in witnesses), (url, witnesses)
    assert isinstance(grade, int) and grade in (0, 1, 2)
    path = RUN / "inputs" / (pid + ".html")
    path.parent.mkdir(exist_ok=True)
    path.write_bytes(html.encode())
    pages.append({"page_id": pid, "id": identifier, "group": group, "url": url, "title": title,
                  "html_path": str(path.relative_to(RUN)), "html_sha256": digest(path.read_bytes()), "source_origin": origin})
    gold.append({"page_id": pid, "group": group, "id": identifier, "url": url, "grade": grade,
                 "reason": reason, "witnesses": witnesses, "expected_publication_date": expected_date,
                 "date_applicable": expected_date is not None, "source_reviewed": True, "origin": origin})


assert not (RUN / "cases.json").exists(), "Never rewrite a frozen cohort"
for group, source, annotations, replay in [
    ("F01", "old-city-control-inputs.json", "old-city-control-gold.json", "old-city-control-v14c"),
    ("F02", "chely-anchor-control-inputs.json", "chely-anchor-control-gold.json", "chely-anchor-control-v14c"),
]:
    base = "runs/news-dates-2026-11-03/"
    data, prior = load(base + source), load(base + annotations)
    previous = load(base + replay + "/result.json")
    labels = {r["id"]: r for r in prior["rows"]}
    entries = {r["id"]: r for r in previous["memory"]["entries"]}
    groups.append({"case": {"id": group, "query": data["query"], "forms": []}, "window": data["window"]})
    for row in data["sources"]:
        html = (ROOT / row["html_path"]).read_bytes().decode("utf-8")
        assert digest(html.encode()) == row["html_sha256"]
        annotation = labels[row["id"]]
        # Source-bound subquotes avoid artificial spaces around inline markup in the independent HTML text view.
        needles = {
            ("F01", "C001"): ["Новый пассажирский поезд №99/100 сообщением Казань — Москва начнёт курсировать", "В пути следования предусмотрены остановки в том числе на станциях Нижний Новгород-Стригино, Владимир."],
            ("F01", "C002"): ["открыли на улице Ракетной в Канавинском районе Нижнего Новгорода"],
            ("F01", "C003"): ["установлены на дорогах Нижегородской области"],
            ("F02", "C020"): ["Столицей какой страны является город Канберра?"],
            ("F02", "C021"): ["на пересечении Копейского шоссе и улицы Машиностроителей", "которые противоречили разметке"],
            ("F02", "C022"): ["В магазинах с невыкупленными товарами можно найти все"],
        }[group, row["id"]]
        date = "2026-11-02" if group == "F02" and row["id"] in ("C020", "C022") else "2026-11-03"
        append(group, row["url"], html, row["title"], annotation["grade"], annotation["reason"], needles,
               {"input": base + source, "html": row["html_path"], "gold": base + annotations, "capture_date": "2026-11-03"}, date)

corpus_path = "feasibility/extraction-selection-corpus-2026-10-30.json"
corpus = load(corpus_path)
for group, old_id, urls, extra in [
    ("F03", "n04", ["https://neon.com/postgresql/tutorial/distinct-on", "https://vonng.com/en/pg/sql-distinct-on/", "https://en.wikipedia.org/wiki/Set_operations_(SQL)"],
     {"https://en.wikipedia.org/wiki/Set_operations_(SQL)": ["Any duplicate records are automatically removed unless UNION ALL is used."]}),
    ("F04", "n02", ["https://docs.python.org/3/library/itertools.html", "https://www.geeksforgeeks.org/python/itertools-groupby-in-python/", "https://www.tutorialspoint.com/python-group-concatenate-till-k"],
     {"https://www.geeksforgeeks.org/python/itertools-groupby-in-python/": ["grouping consecutive elements of an iterable that have the same value"],
      "https://www.tutorialspoint.com/python-group-concatenate-till-k": ["Group concatenate till K means joining elements within a sequence until encountering a specific delimiter value K."]}),
]:
    old_case = next(r for r in corpus["pool"]["cases"] if r["case"]["id"] == old_id)
    old_gold = next(r for r in corpus["gold"]["cases"] if r["id"] == old_id)
    groups.append({"case": {"id": group, "query": old_case["case"]["query"], "forms": []}, "window": None})
    for url in urls:
        annotation = next(r for r in old_gold["gold"] if r["url"] == url)
        snapshot = corpus["raw_snapshots"][Path(annotation["snapshot"]).name]
        raw = snapshot["fetched"]
        assert raw["status"] == 200 and raw.get("html")
        candidate = next(c for c in old_case["candidates"] if c["url"] == url)
        needles = extra.get(url) or {
            "https://neon.com/postgresql/tutorial/distinct-on": ["unique entry becomes unpredictable", "without the ORDER BY clause"],
            "https://vonng.com/en/pg/sql-distinct-on/": ["unless the query is sorted on enough columns"],
            "https://docs.python.org/3/library/itertools.html": ["returns consecutive keys and groups", "shares the underlying iterable", "the previous group is no longer visible"],
        }[url]
        append(group, raw["url"], raw["html"], candidate["title"], annotation["grade"], annotation["basis"], needles,
               {"corpus": corpus_path, "snapshot": annotation["snapshot"], "capture_date": "2026-10-30", "requested_url": url, "hops": raw["hops"]})

assert len(pages) == len({p["url"] for p in pages}) == 12
assert sum(r["grade"] == 2 for r in gold) == 7 and sum(r["date_applicable"] for r in gold) == 6
cases = {"schema": "firecrawl-cached-reader-v1", "scope": "source-aware historical fixed-URL component A/B, no retrieval/initial classifier", "groups": groups, "pages": pages}
save(RUN / "cases.json", cases)
save(RUN / "gold.json", {"schema": "firecrawl-reader-gold-v1", "frozen_before_reader_outcomes": True,
     "rows": gold, "limits": ["Prior source judgments reused before new outcomes; same author study, not independent acceptance.", "Historical snapshots Oct30/Nov3, not fresh Nov5 news or URL discovery recall.", "Quote preservation uses exact case-sensitive substring after whitespace folding, separately from semantic final direct."]})
save(RUN / "cohort-freeze.json", {"client_date": "2026-11-05", "source_pages": 12, "positive_pages": 7, "date_pages": 6,
     "files": {str(p.relative_to(ROOT)): digest(p.read_bytes()) for p in [RUN / "cases.json", RUN / "gold.json", *sorted((RUN / "inputs").glob("*.html"))]},
     "mode": "real upstream parse uploadedHTML; common plain-text serializer on cleanedHTML; rawHtml must hash-match input; no external target reads"})
print("Frozen 12 real source pages: 7 positive, 5 negative/background, 6 applicable publication dates")
