"""Posthoc NEW24 gate diagnostics; reads frozen gold and completed results only."""
import argparse
import collections
import copy
import hashlib
import json
import runpy
from pathlib import Path

ROOT = Path('runs/admission-literals-2026-10-30')
GOLD = Path('feasibility/admission-literals-gold-v1.json')
DEST = Path('feasibility/admission-literals-misses-2026-10-30.json')
BRANCHES = ('baseline', 'fixed')
HELPER = Path(__file__).with_name('analyze-extraction-selection-2026-10-30.py')
WITNESS_ERRORS = {'invalid_quote_length', 'quote_not_in_cited_source',
                  'missing_subject_binding', 'heading_only_core'}


def sha(body):
    return hashlib.sha256(body).hexdigest()


def load(path):
    return json.loads(Path(path).read_text())


def predicted(row):
    return {r['id'] for r in row['results'] if r['classification'] == 'direct'}


def payloads(row):
    """Use structured audit_targets, not a guess based on call count or prompt text."""
    calls = row.get('model_calls')
    if not isinstance(calls, list):
        return [], False
    out, complete = [], True
    for i, call in enumerate(calls):
        request = call.get('input') if isinstance(call, dict) else None
        messages = request.get('messages') if isinstance(request, dict) else None
        if not isinstance(messages, list) or any(not isinstance(m, dict) for m in messages):
            complete = False
            continue
        users = [m.get('content') for m in messages if m.get('role') == 'user']
        if not users:
            complete = False
        for content in users:
            try:
                value = json.loads(content)
            except (TypeError, ValueError):
                complete = False
                continue
            if not isinstance(value, dict):
                complete = False
                continue
            out.append((i, value, sha(content.encode())))
    return out, complete


def diagnose(row, ident, gold):
    verification = row.get('page_verification')
    verification = verification if isinstance(verification, dict) else {}
    memory = verification.get('evidence_memory')
    valid_pool = isinstance(memory, dict) and isinstance(memory.get('entries'), list) \
        and isinstance(memory.get('version'), str) and len(memory['entries']) <= 10 \
        and all(isinstance(e, dict) and isinstance(e.get('id'), str) for e in memory['entries']) \
        and len({e['id'] for e in memory['entries']}) == len(memory['entries'])
    entries = memory['entries'] if valid_pool else []
    entry = next((e for e in entries if e.get('id') == ident), None)
    documents = verification.get('documents')
    valid_records = isinstance(documents, list) and all(isinstance(d, dict) for d in documents)
    records = documents if valid_records else []
    record = next((d for d in records if d.get('id') == ident), None)
    calls, complete_calls = payloads(row)
    audit_calls = [i for i, p, _ in calls if isinstance(p.get('audit_targets'), list)
                   and ident in p['audit_targets']]
    coverage = [(i, p, h) for i, p, h in calls if 'fixed_subject' in p
                and isinstance(p.get('candidates'), list)
                and any(isinstance(c, dict) and c.get('id') == ident for c in p['candidates'])]
    supplied = next((c.get('passages') for _, p, _ in reversed(coverage)
                     for c in p['candidates'] if isinstance(c, dict) and c.get('id') == ident), None)
    passages = entry.get('passages') if entry else None
    checks = entry.get('checks') if entry else None
    requirements = memory.get('requirements') if valid_pool else None
    valid_checks = isinstance(requirements, list) and 1 <= len(requirements) <= 4 \
        and isinstance(checks, list) and len(checks) == 4 \
        and all(isinstance(c, dict) for c in checks)
    statuses = [c.get('status') for c in checks[:len(requirements)]] if valid_checks else None
    all_supported = statuses is not None and all(s == 'supported' for s in statuses)
    gaps_map = memory.get('unresolved_literals') if valid_pool else None
    gaps = gaps_map.get(ident) if isinstance(gaps_map, dict) else None
    rejections = verification.get('rejected')
    rejected = rejections.get(ident) if isinstance(rejections, dict) else None
    status_gate = isinstance(rejected, str) and 'source_status:' in rejected
    witness_rejections = [r for r in memory.get('witness_rejections', [])
                          if r.get('id') == ident] if valid_pool else []
    conflicts = [r for r in memory.get('audit_conflicts', [])
                 if ident in (r.get('left'), r.get('right'))] if valid_pool else []
    invalid_conflicts = [r for r in memory.get('witness_rejections', [])
                         if r.get('reason') == 'invalid_conflict_witness'
                         and ident in (r.get('left'), r.get('right'))] if valid_pool else []
    quote_presence = None
    if isinstance(supplied, list) and all(isinstance(p, dict) and isinstance(p.get('text'), str)
                                          for p in supplied):
        texts = [' '.join(p['text'].split()) for p in supplied]
        quote_presence = [{'quote':s['quote'], 'exact_in_supplied_passage':
                           any(' '.join(s['quote'].split()) in text for text in texts)}
                          for s in gold.get('support', [])]
    facts = dict(in_coverage_pool=entry is not None if valid_pool else None,
                 coverage_pool_position=next((i + 1 for i, e in enumerate(entries)
                                              if e.get('id') == ident), None),
                 read_attempted=entry.get('read_attempted') if entry else None,
                 read_record=record, selected_document_ids=[d.get('id') for d in records],
                 supplied_passages=passages, coverage_statuses=statuses,
                 checks=checks, literal_gaps=gaps, evidence_rejection=rejected,
                 witness_rejections=witness_rejections, audit_conflicts=conflicts,
                 invalid_conflict_witnesses=invalid_conflicts,
                 audit_supported=entry.get('audit_supported') if entry else None,
                 audit_reason=entry.get('audit_reason') if entry else None,
                 audit_call_indexes=audit_calls, model_call_inputs_complete=complete_calls,
                 coverage_call_indexes=[i for i, _, _ in coverage],
                 coverage_input_sha256=[h for _, _, h in coverage],
                 gold_quote_presence=quote_presence, row_status=row.get('status'),
                 row_errors=row.get('errors'))

    def result(stage, evidence, uncertainty=None):
        return dict(stage=stage, evidence=evidence, uncertainty=uncertainty, observations=facts)

    if ident in predicted(row):
        return result('published_direct', ['results explicitly publishes this candidate as direct'])
    if not valid_pool:
        return result('unknown_stage', [], 'No explicit complete evidence-memory pool diagnostics')
    if entry is None:
        return result('outside_coverage_pool', ['candidate absent from the recorded evidence entries'])
    if entry.get('audit_supported') is True:
        if all_supported and gaps == [] and not status_gate:
            return result('final_policy_fallback', ['all recorded coverage checks supported',
                          'literal diagnostics empty and audit_supported=true',
                          'candidate absent from final direct set'],
                          'Only publication boundary proven; exact downstream selection/fallback cause unknown')
        return result('unknown_stage', [], 'Accepted audit conflicts with incomplete gate diagnostics')
    if record and record.get('error'):
        return result('read_error', [f"document record error: {record['error']}"])
    if record and isinstance(record.get('http_status'), int) and record['http_status'] != 200:
        return result('read_status', [f"document HTTP status: {record['http_status']}"])
    if entry.get('read_attempted') is False:
        if valid_records and len(records) == 4 and record is None:
            return result('not_selected_for_four_reads', ['read_attempted=false',
                          'four other selected-document records completed'])
        return result('unknown_stage', ['read_attempted=false'],
                      'Fewer than four complete read records; cannot distinguish selection from early abort')
    if entry.get('read_attempted') is not True or record is None:
        return result('unknown_stage', [], 'Read outcome not explicitly recorded for this candidate')
    if not isinstance(passages, list) or any(not isinstance(p, dict) or 'field' not in p for p in passages):
        return result('unknown_stage', [], 'Supplied-passage fields are incomplete')
    if not any(p['field'] == 'body' for p in passages):
        return result('no_body_passages', ['candidate passages explicitly contain no body field'],
                      'Does not distinguish extraction loss from passage selection')
    if not valid_checks:
        if memory.get('reviewed_rounds') == 0 and checks == [] and requirements == []:
            return result('coverage_uncommitted', ['reviewed_rounds=0 and empty checks/requirements'],
                          'No valid coverage committed; request/parse/budget cause is not inferred')
        return result('unknown_stage', [], 'Required coverage-check diagnostics are incomplete')
    invalid = [r for r in witness_rejections if r.get('reason') in WITNESS_ERRORS
               and isinstance(r.get('check'), int) and 0 <= r['check'] < len(statuses)
               and statuses[r['check']] != 'supported']
    if invalid and memory.get('reviewed_rounds') == 1:
        return result('coverage_witness_rejected', [r['reason'] for r in invalid])
    if not all_supported:
        if any(s == 'ambiguous' or s not in ('supported', 'missing', 'contradicted') for s in statuses):
            return result('unknown_stage', ['coverage includes ambiguous/unrecognized check status'],
                          'No current exact-witness rejection establishes why it is ambiguous')
        return result('coverage_not_supported', [f'R{i}: {s}' for i, s in enumerate(statuses)
                                                 if s != 'supported'],
                      'Reviewer verdict observed; fragment loss or semantic reason not inferred')
    if not isinstance(gaps, list):
        return result('unknown_stage', [], 'Literal diagnostics missing; empty is not assumed')
    if gaps:
        return result('literal_gate', ['all required coverage statuses supported',
                                      f'explicit unresolved literals: {gaps}'])
    if status_gate:
        return result('source_status_gate', [rejected])
    audit = entry.get('audit_supported', 'missing')
    if audit is False:
        reason = entry.get('audit_reason')
        if conflicts or invalid_conflicts or isinstance(reason, str) and 'unresolved_source_conflict' in reason:
            return result('audit_conflict_quarantine', [reason or 'explicit conflict record'],
                          'Invalid conflict quotes are quarantine evidence, not proof of a real source conflict')
        return result('audit_rejected', ['audit_supported=false', reason or 'reason unspecified'])
    if audit is None:
        if audit_calls:
            return result('audit_uncommitted', ['audit request explicitly targeted this candidate',
                          'audit_supported=null'], 'No committed audit verdict; no rejection inferred')
        if complete_calls:
            return result('audit_not_run', ['complete recorded inputs contain no audit target for this candidate'],
                          'Reason for not targeting candidate is not inferred')
    return result('unknown_stage', [], 'Audit state/call history is incomplete')


def main():
    freeze_path = ROOT / 'gold-freeze.json'
    freeze = load(freeze_path)
    assert freeze.get(str(GOLD)) == sha(GOLD.read_bytes()), 'gold is not the frozen version'
    gold = load(GOLD)
    assert len(gold['cases']) == 24
    branches = {b: load(ROOT / b / 'results.json') for b in BRANCHES}
    rows = {b: {r['id']: r for r in data['cases']} for b, data in branches.items()}
    gold_rows = {r['id']: r for r in gold['cases']}
    assert len(gold_rows) == 24, 'gold case IDs must be unique'
    assert all(len(data['cases']) == 24 for data in branches.values()), 'complete unique NEW24 cases required'
    assert all(set(r) == set(gold_rows) for r in rows.values()), 'both completed NEW24 arms required'
    metrics = runpy.run_path(str(HELPER), run_name='diagnostic_helpers')['metrics']
    report = dict(kind='posthoc-admission-literals-gate-diagnostics',
                  scope='Observed candidate gate boundaries on frozen NEW24 source gold; no runtime or gold tuning.',
                  false_negatives={b: [] for b in BRANCHES}, false_directs={b: [] for b in BRANCHES},
                  unknown_directs={b: [] for b in BRANCHES}, stage_counts={}, metrics={},
                  provenance={'gold':dict(path=str(GOLD), sha256=sha(GOLD.read_bytes())),
                              'gold_freeze':dict(path=str(freeze_path), sha256=sha(freeze_path.read_bytes())),
                              'script':dict(path=str(Path(__file__)), sha256=sha(Path(__file__).read_bytes())),
                              'metrics_helper':dict(path=str(HELPER), sha256=sha(HELPER.read_bytes())),
                              'results':{b:dict(path=str(ROOT / b / 'results.json'),
                                               sha256=sha((ROOT / b / 'results.json').read_bytes())) for b in BRANCHES}},
                  limits=['Stages are recorded deficits/opportunities, not proof fixing one gate yields direct.',
                          'Missing fields and unresolved ambiguity remain unknown, not inferred negative verdicts.',
                          'A missing exact gold quote in supplied context alone does not prove extraction/selector loss: source/runtime normalization can differ.',
                          'This report does not alter source grades, primary metrics, code or model configuration.'],
                  network_calls=0, model_calls=0, runtime_edits=0, gold_edits=0)
    for branch, case_rows in rows.items():
        samples = []
        for cid, row in case_rows.items():
            source = gold_rows[cid]
            assert row['query'] == source['query']
            grades = {g['id']: g for g in source['gold']}
            cards = {c['id']: c for c in row['candidates']}
            assert len(grades) == len(source['gold']) and len(cards) == len(row['candidates']), 'unique candidate IDs required'
            assert grades.keys() == cards.keys()
            assert all(g['url'] == cards[i]['url'] for i, g in grades.items())
            direct = predicted(row)
            assert direct <= grades.keys()
            samples.append((grades, direct))
            for ident, g in grades.items():
                group = 'false_negatives' if g['grade'] == 2 and ident not in direct else \
                    'false_directs' if g['grade'] in (0, 1) and ident in direct else \
                    'unknown_directs' if g['grade'] is None and ident in direct else None
                if group:
                    report[group][branch].append(dict(case=cid, id=ident, query=row['query'],
                        url=g['url'], gold_grade=g['grade'], gold_basis=g['basis'],
                        gold_source_snapshot=g.get('snapshot'), gold_html_hash=g.get('html_hash'),
                        gold_support=g.get('support', []), gold_partial_support=g.get('partial_support', []),
                        gold_negative_support=g.get('negative_support', []),
                        **diagnose(row, ident, g)))
        report['metrics'][branch] = metrics(samples)
        report['stage_counts'][branch] = dict(collections.Counter(r['stage'] for r in report['false_negatives'][branch]))
    fp_ids = {b: {(r['case'], r['id']) for r in report['false_directs'][b]} for b in BRANCHES}
    report['false_direct_changes'] = {label: [dict(case=c, id=i) for c, i in sorted(ids)]
        for label, ids in [('new', fp_ids['fixed'] - fp_ids['baseline']),
                           ('removed', fp_ids['baseline'] - fp_ids['fixed']),
                           ('persisted', fp_ids['baseline'] & fp_ids['fixed'])]}
    DEST.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(dict(output=str(DEST), metrics=report['metrics'], stage_counts=report['stage_counts'],
                          false_direct_changes=report['false_direct_changes']), ensure_ascii=False))


def self_check():
    entry = dict(id='C001', read_attempted=True, passages=[dict(id='B1', field='body')],
                 checks=[dict(status='supported', passages=['B1'], quote='A concrete witness statement.') for _ in range(4)],
                 audit_supported=None, audit_reason='')
    row = dict(results=[], model_calls=[], page_verification=dict(documents=[dict(id='C001', http_status=200)],
               rejected={}, evidence_memory=dict(version='request-evidence-test', entries=[entry],
               requirements=[dict(id='R0')], reviewed_rounds=1, unresolved_literals={'C001': []},
               witness_rejections=[], audit_conflicts=[])))
    grade = dict(grade=2, support=[])

    def check(expected, change):
        test = copy.deepcopy(row)
        change(test)
        actual = diagnose(test, 'C001', grade)['stage']
        assert actual == expected, (expected, actual)

    def e(test):
        return test['page_verification']['evidence_memory']['entries'][0]

    check('outside_coverage_pool', lambda r: r['page_verification']['evidence_memory'].update(entries=[]))
    check('unknown_stage', lambda r: r['page_verification'].update(evidence_memory={}))
    check('unknown_stage', lambda r: r.update(page_verification=None))
    check('unknown_stage', lambda r: e(r).update(read_attempted=False))

    def four_reads(r):
        e(r)['read_attempted'] = False
        r['page_verification']['documents'] = [dict(id=f'C{i:03}') for i in range(2, 6)]

    check('not_selected_for_four_reads', four_reads)
    check('read_error', lambda r: r['page_verification']['documents'][0].update(error='fetch_timeout'))
    check('read_status', lambda r: r['page_verification']['documents'][0].update(http_status=404))
    check('no_body_passages', lambda r: e(r).update(passages=[dict(id='S', field='snippet')]))
    def no_coverage(r):
        e(r)['checks'] = []
        r['page_verification']['evidence_memory'].update(requirements=[], reviewed_rounds=0)

    check('coverage_uncommitted', no_coverage)
    check('coverage_not_supported', lambda r: e(r)['checks'][0].update(status='missing'))
    check('unknown_stage', lambda r: e(r)['checks'][0].update(status='ambiguous'))

    def witness(r):
        e(r)['checks'][0].update(status='ambiguous', quote='')
        r['page_verification']['evidence_memory']['witness_rejections'] = [dict(id='C001', check=0, reason='heading_only_core')]

    check('coverage_witness_rejected', witness)
    check('literal_gate', lambda r: r['page_verification']['evidence_memory']['unresolved_literals'].update(C001=['TypedName']))
    check('unknown_stage', lambda r: r['page_verification']['evidence_memory'].pop('unresolved_literals'))
    check('source_status_gate', lambda r: r['page_verification']['rejected'].update(C001='source_status:unsupported version'))
    check('audit_rejected', lambda r: e(r).update(audit_supported=False, audit_reason='Missing actual relation'))
    check('audit_conflict_quarantine', lambda r: e(r).update(audit_supported=False, audit_reason='unresolved_source_conflict'))
    check('audit_not_run', lambda r: None)

    def audit_call(r):
        r['model_calls'] = [dict(input=dict(messages=[dict(role='user', content=json.dumps(dict(audit_targets=['C001'])))]))]

    check('audit_uncommitted', audit_call)
    check('final_policy_fallback', lambda r: e(r).update(audit_supported=True))
    check('published_direct', lambda r: r.update(results=[dict(id='C001', classification='direct')]))
    metrics = runpy.run_path(str(HELPER), run_name='diagnostic_helpers')['metrics']
    measured = metrics([({'P':dict(grade=2), 'N':dict(grade=1), 'U':dict(grade=None)}, {'N', 'U'})])
    assert (measured['tp'], measured['fp'], measured['fn'], measured['unjudged_direct']) == (0, 1, 1, 1)
    print('diagnostic self-check passed: explicit gate boundaries, ambiguity, audit null, published direct, known/unknown metrics')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-check', action='store_true')
    if parser.parse_args().self_check:
        self_check()
    else:
        main()
