#!/usr/bin/env python3
"""Freeze a URL-disjoint historical component cohort before new inference."""
import hashlib
import html
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / "runs/qwen-firecrawl-2026-11-05"
PRIOR = ROOT / "runs/news-dates-2026-11-03/confirm/blind"


def load(path):
    return json.loads(path.read_text())


def fold(text):
    return " ".join(text.split())


def sha(data):
    return hashlib.sha256(data).hexdigest()


assert fold("one\n two") == "one two"
assert not (RUN / "cases.json").exists(), "Never rewrite the frozen cohort"
documents = load(PRIOR / "source-native-documents.json")["documents"]
old_gold = [row for name in ["gold-ca01-ca03.json", "gold-ca04-ca06.json"]
            for row in load(PRIOR / name)["rows"]]
old_urls = {p["url"] for p in load(ROOT / "runs/firecrawl-2026-11-05/cases.json")["pages"]}
window = load(ROOT / "runs/firecrawl-2026-11-05/cases.json")["groups"][0]["window"]
groups, pages, gold = [], [], []
spec = [
    ("Q01", "новости Перми", [
        ("https://59.ru/text/gorod/2026/11/03/76741257/", 2,
         ["В Орджоникидзевском районе Перми продолжают благоустраивать сквер на улице Трясолобова.", "Объект готов на 70%."]),
        ("https://59.ru/text/criminal/2026/11/03/76741420/", 2,
         ["В Перми суд приговорил мошенницу к пяти годам колонии.", "Всего за такие услуги 470 пермяков заплатили больше 41 миллиона рублей."]),
        ("https://59.ru/text/longread/2026/05/21/76426099/", 0,
         ["В 2026 году, объявленном в Перми Годом промышленности"]),
    ]),
    ("Q02", "последние новости Уфы", [
        ("https://ufa1.ru/text/business/2026/11/03/76740975/", 2,
         ["Торговцы очередного «народного рынка» в Уфе жалуются на его закрытие.", "Оснований для отмены предписаний нет"]),
        ("https://ufa1.ru/text/gorod/2026/11/02/76720417/", 2,
         ["Следующая часть нашего обзора о депутатах Горсовета", "Сейчас он заместитель председателя Горсовета Уфы"]),
        ("https://ufa1.ru/text/world/2026/11/02/76740698/", 0,
         ["Сегодня подводим итоги 2 ноября.", "Министерство финансов подготовило законопроект"]),
    ]),
    ("Q03", "новости Екатеринбурга", [
        ("https://www.e1.ru/text/incidents/2026/11/03/76741416/", 2,
         ["Появилось видео момента утреннего ДТП, произошедшего у кольца на улице Малышева.", "К счастью, в аварии никто не пострадал."]),
        ("https://www.e1.ru/text/realty/2026/11/03/76741060/", 2,
         ["Госкорпорация «Дом.рф» выставила на торги еще один памятник в Екатеринбурге.", "Аукцион за него начнется с начальной цены в 15 275 000 рублей."]),
        ("https://www.e1.ru/", 1,
         ["В Екатеринбурге скандальный стример прокатил полуголую пассажирку"]),
    ]),
    ("Q04", "новости Омска", [
        ("https://superomsk.ru/news/167842-v_omske_rastet_chislo_ofitsialnykh_teatrov/", 2,
         ["Основание юрлица омского театра оказалось с нюансом.", "театр обрел в год, когда Омск стал культурной столицей страны."]),
        ("https://superomsk.ru/news/167820-nazvany_daty_reisov_iz_omska_v_krasnodar_na_zimnie_kanikuly/", 2,
         ["Дополнительные рейсы будут выполняться по понедельникам и вторникам.", "По вторникам – 22 и 29 декабря, 5 и 12 января планируются рейсы из Омска в Норильск."]),
        ("https://www1.ru/articles/2026/11/03/ii-razognal-rovery-iandeksa-na-43-kak-ustroeny-roboty-dostavshhiki-i-kto-reguliruet-ix-dvizenie.html", 0,
         ["роверы «Яндекса» увеличили среднюю скорость с 3,5 до 5 км/ч"]),
    ]),
]
for gid, query, sources in spec:
    groups.append({"case": {"id": gid, "query": query, "forms": []}, "window": window})
    for i, (url, grade, witnesses) in enumerate(sources, 1):
        assert url not in old_urls
        annotation = next(v for v in old_gold if v["url"] == url)
        variant = documents[url]["variants"][0]
        source = ROOT / variant["raw_html_path"]
        raw = source.read_bytes()
        assert sha(raw) == variant["html_hash"] and variant["http_status"] == 200
        raw_text = raw.decode("utf-8")
        assert raw_text.encode() == raw
        assert all(fold(w) in fold(variant["full_text"]) for w in witnesses), url
        pid, identifier = f"P{len(pages)+1:02d}", f"C{i:03d}"
        target = RUN / "inputs" / f"{pid}.html"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(raw)
        title = re.search(r"<title[^>]*>(.*?)</title>", raw_text, re.I | re.S)
        title = fold(html.unescape(title.group(1))) if title else ""
        origin = {"bundle": str((PRIOR / "source-native-documents.json").relative_to(ROOT)),
                  "html": str(source.relative_to(ROOT)), "html_sha256": sha(raw),
                  "prior_gold_case": annotation["case_id"], "prior_grade": annotation["grade"],
                  "capture_date": "2026-11-03", "variant_id": variant["variant_id"]}
        pages.append({"page_id": pid, "id": identifier, "group": gid, "url": url,
                      "title": title, "html_path": str(target.relative_to(RUN)),
                      "html_sha256": sha(raw), "source_origin": origin})
        reason = annotation["reason"]
        if pid == "P12":
            reason = "Own technology story about delivery robots has no Omsk event/place relation; global topic and publisher cannot establish the city."
        gold.append({"page_id": pid, "group": gid, "id": identifier, "url": url,
                     "grade": grade, "reason": reason, "witnesses": witnesses,
                     "expected_publication_date": annotation.get("published_date"),
                     "expected_kind": annotation["kind"], "date_applicable": annotation["kind"] == "article",
                     "source_reviewed": True, "origin": origin})
assert len(pages) == len({p["url"] for p in pages}) == 12
assert sum(v["grade"] == 2 for v in gold) == 8
assert not old_urls.intersection(p["url"] for p in pages)
cases = {"schema": "firecrawl-cached-reader-v1", "comparison": "qwen-selector",
         "scope": "URL-disjoint relative to previous Firecrawl12; historical source-aware component, no retrieval or initial classifier",
         "groups": groups, "pages": pages}
for name, value in [("cases.json", cases), ("gold.json", {"frozen_before_new_inference": True, "rows": gold})]:
    (RUN / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
(RUN / "cohort-freeze.json").write_text(json.dumps({
    "pages": 12, "positive_pages": 8, "negative_background_pages": 4,
    "disjoint_from_previous_firecrawl_urls": True, "source_capture_date": "2026-11-03",
    "source_aware": True, "files": {str(p.relative_to(ROOT)): sha(p.read_bytes()) for p in
        [RUN / "cases.json", RUN / "gold.json", *sorted((RUN / "inputs").glob("*.html"))]},
}, indent=2) + "\n")
print(f"Frozen NEW12: 8 positives, 4 controls, {sum(len(v['witnesses']) for v in gold)} witnesses, no previous Firecrawl URL")
