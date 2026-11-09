#!/usr/bin/env python3
"""Export only public observables from the frozen model comparison. No model calls."""
import argparse
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATASET = ROOT / 'fixtures/experiments/strengthened-10-20261106'
SOURCE_FILES = ['document.md', 'bicycle-workshop.md', 'language-cafe.md', 'community-cooking.md', 'walking.md'] + [f'expanded-10-20261105/activity-{n:03}.md' for n in range(6, 11)]
MODELS = {'gpt-6.1-sol': 'Sol', 'gpt-6-luna': 'Luna'}
EXPECTED_MANIFEST_SHA256 = 'c1385fc9aac9d6c59671971cc8a09e14d9628bc131dc75e26ca20800579e1157'


def read_json(path):
    return json.loads(Path(path).read_text())


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def ref_key(ref):
    return ref['source_id'] + ':' + ref['section_id']


def is_complete(offered, alternatives):
    return any(set(option).issubset(offered) for option in alternatives)


def read_sources():
    documents, sections = [], {}
    for name in SOURCE_FILES:
        text = (ROOT / 'fixtures' / name).read_text()
        title = re.search(r'^# (.+)$', text, re.M).group(1)
        source_id = re.search(r'^ID: (.+)$', text, re.M).group(1)
        date = re.search(r'^Brondatum: (.+)$', text, re.M).group(1)
        pieces = re.split(r'^## (S\d+) — (.+)\n', text, flags=re.M)
        preamble = pieces[0].split(date, 1)[1].strip()
        document = {'id': source_id, 'title': title, 'date': date, 'preamble': preamble, 'sections': []}
        for i in range(1, len(pieces), 3):
            section = {'source_id': source_id, 'source_title': title, 'source_date': date, 'id': pieces[i], 'heading': pieces[i + 1], 'text': pieces[i + 2].strip()}
            key = source_id + ':' + section['id']
            if key in sections:
                raise ValueError('Duplicate source identity')
            sections[key] = section
            document['sections'].append(key)
        documents.append(document)
    return documents, sections


def project_run(native, assessment, sources):
    """Whitelist observable fields, checking actual offers rather than candidate lists."""
    alternatives = [[ref_key(ref) for ref in option] for option in assessment['source_coverage']['expected_source_options']]
    if len(native['tool_calls']) != assessment['actual_search_calls'] or len(assessment['calls']) != assessment['actual_search_calls']:
        raise ValueError('Tool call count differs from assessment')
    calls, cumulative, first_complete = [], set(), None
    for raw, grade in zip(native['tool_calls'], assessment['calls'], strict=True):
        if raw['query'] != grade['query'] or raw['status'] != grade['status']:
            raise ValueError('Observable call differs from assessment')
        offered = [ref_key(ref) for ref in grade['offered_sections']]
        hits = []
        for hit in raw['result']['hits']:
            actual = hit['section']
            key = actual['source_id'] + ':' + actual['id']
            if key not in sources or actual != sources[key]:
                raise ValueError('Offered source text is not canonical')
            hits.append(key)
        if hits != offered:
            raise ValueError('Candidate or unoffered source was included')
        cumulative.update(hits)
        complete = is_complete(cumulative, alternatives)
        if complete != grade['complete_expected_sources']:
            raise ValueError('Cumulative source coverage differs')
        if complete and first_complete is None:
            first_complete = grade['index']
        calls.append({'index': grade['index'], 'query': raw['query'], 'hits': hits, 'complete': complete})
    complete = is_complete(cumulative, alternatives)
    if complete != assessment['source_coverage']['complete'] or first_complete != assessment['source_coverage']['first_complete_call_index']:
        raise ValueError('Session source coverage differs')
    return {'complete': complete, 'actions': len(calls), 'first_complete': first_complete, 'calls': calls}


def export(experiment_dir):
    experiment_dir = Path(experiment_dir).resolve()
    if digest(experiment_dir / 'EXECUTION_MANIFEST.json') != EXPECTED_MANIFEST_SHA256:
        raise ValueError('This is not the reviewed execution manifest')
    manifest = read_json(experiment_dir / 'EXECUTION_MANIFEST.json')
    if manifest['new_attempts'] != 60 or manifest['old_aborted_attempts'] != 6:
        raise ValueError('Expected the distinct 60-session replacement cohort')
    for item in manifest['files']:
        path = (experiment_dir / item['path']).resolve()
        if not path.is_relative_to(experiment_dir) or digest(path) != item['sha256']:
            raise ValueError('Sealed evidence changed')
    index = read_json(experiment_dir / 'ANALYSIS_INDEX.json')
    if len(index['attempts']) != 60 or index['ai_judge_calls'] != 0:
        raise ValueError('Wrong experiment cohort')
    documents, sources = read_sources()
    questions = read_json(DATASET / 'questions.json')
    refs = {item['case_id']: item for item in read_json(DATASET / 'reference-details.json')['cases']}
    labels = {item['case_id']: item for item in read_json(DATASET / 'expectations.json')['cases']}
    cases = {}
    for item in questions['cases']:
        case_id = item['id']
        alternatives = [[ref_key(ref) for ref in plan] for plan in refs[case_id]['expected_source_plans']]
        cases[case_id] = {'id': case_id, 'question': item['question'], 'purpose': labels[case_id]['facts'][0]['description'], 'alternatives': alternatives, 'runs': []}
    assessment_files = {}
    seen = set()
    for attempt in index['attempts']:
        key = (attempt['model'], attempt['case_id'], attempt['trial'])
        if key in seen or attempt['model'] not in MODELS or attempt['trial'] not in (1, 2, 3):
            raise ValueError('Duplicate or unplanned model execution')
        seen.add(key)
        record_path = Path(attempt['record_path']).resolve()
        if not record_path.is_relative_to(experiment_dir) or digest(record_path) != attempt['record_sha256']:
            raise ValueError('Execution record binding changed')
        wrapper = json.loads(record_path.read_text().strip())
        native = wrapper['live']['run']
        if wrapper['dataset_id'] != 'strengthened-10-20261106' or wrapper['question'] != cases[attempt['case_id']]['question'] or wrapper['requested_model'] != attempt['model'] or native is None or native['model'] != attempt['model'] or native['status'] != 'completed':
            raise ValueError('Wrong question/model or unassessed run')
        assessment_path = Path(attempt['assessment_path']).resolve()
        if not assessment_path.is_relative_to(experiment_dir):
            raise ValueError('Assessment outside experiment')
        if assessment_path not in assessment_files:
            assessment_files[assessment_path] = {row['run_id']: row for row in map(json.loads, assessment_path.read_text().splitlines())}
        assessment = assessment_files[assessment_path][wrapper['live']['run_id']]
        if assessment['status'] != 'scored':
            raise ValueError('Expected a native scored run')
        projected = project_run(native, assessment, sources)
        actual_options = [[ref_key(ref) for ref in option] for option in assessment['source_coverage']['expected_source_options']]
        if sorted(tuple(sorted(option)) for option in actual_options) != sorted(tuple(sorted(option)) for option in cases[attempt['case_id']]['alternatives']):
            raise ValueError('Native source alternatives differ from frozen references')
        projected.update({'model': attempt['model'], 'trial': attempt['trial']})
        cases[attempt['case_id']]['runs'].append(projected)
    if len(seen) != 60 or any(len(case['runs']) != 6 for case in cases.values()):
        raise ValueError('Missing comparison cells')
    models = []
    for model, label in MODELS.items():
        runs = [run for case in cases.values() for run in case['runs'] if run['model'] == model]
        models.append({'id': model, 'label': label, 'attempts': len(runs), 'complete': sum(run['complete'] for run in runs), 'actions': sum(run['actions'] for run in runs), 'always_complete': sum(all(run['complete'] for run in case['runs'] if run['model'] == model) for case in cases.values())})
    for case in cases.values():
        case['runs'].sort(key=lambda run: (run['model'], run['trial']))
    payload = {'schema_version': 1, 'evidence_manifest_sha256': EXPECTED_MANIFEST_SHA256, 'dataset': 'strengthened-10-20261106', 'measured_date': '2026-11-06', 'source_date': questions['as_of'], 'corpus_sha256': questions['corpus_sha256'], 'method': 'dense', 'method_label': 'Op betekenis', 'repetitions': 3, 'answer_quality': 'not_assessed', 'historical_aborted_attempts': 6, 'models': models, 'documents': documents, 'sources': sources, 'questions': list(cases.values())}
    serialized = json.dumps(payload, ensure_ascii=False)
    for forbidden in ('/Users/', '/private/tmp/', 'access_token', 'runtime_path', 'reasoning'):
        if forbidden in serialized:
            raise ValueError('Non-public content in projected observables')
    return payload


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--experiment-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT / 'static/site/results.json')
    args = parser.parse_args()
    data = export(args.experiment_dir)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n')
    print(f'Exported {len(data["questions"])} questions, 60 sessions, 10 documents.')


if __name__ == '__main__':
    main()
