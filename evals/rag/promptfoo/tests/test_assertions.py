"""Behavioral checks for the new canonical promptfoo retrieval assertions."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import unittest

HERE = Path(__file__).resolve().parents[1]
ROOT = HERE.parents[2]


def load_assertions():
    path = HERE / 'assertions.py'
    if not path.exists():
        return None
    spec = importlib.util.spec_from_file_location('coa_rag_assertions', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def section(source_file, section_id):
    text = (ROOT / 'fixtures' / source_file).read_text()
    pieces = re.split(r'^## (S\d+) — (.+)\n', text, flags=re.M)
    blocks = {pieces[i]: (pieces[i+1], pieces[i+2].strip()) for i in range(1, len(pieces), 3)}
    return {
        'source_id': re.search(r'^ID: (.+)$', text, re.M)[1],
        'source_title': re.search(r'^# (.+)$', text, re.M)[1],
        'source_date': re.search(r'^Brondatum: (.+)$', text, re.M)[1],
        'id': section_id, 'heading': blocks[section_id][0], 'text': blocks[section_id][1],
    }


def local_record(keys=('S2', 'S3'), case_id='P01'):
    questions = json.loads((ROOT/'evals/shared/datasets/legacy/questions.json').read_text())['cases'] + json.loads((ROOT/'evals/shared/datasets/legacy/heldout-questions.json').read_text())['cases']
    case = next(c for c in questions if c['id'] == case_id)
    record = json.loads((ROOT/'evals/rag/fixtures/regressions/t02-dense-search.json').read_text())
    record.update(run_id='measured-run-one',case_id=case_id,question=case['question'],family=case['family'],split=case['split'],kind=case['kind'])
    hits=[]; candidates=[]
    for i,key in enumerate(keys):
        source=section('document.md',key)
        scores={'dense_similarity':.9-i*.1,'bm25_score':None,'dense_rank':i+1,'keyword_rank':None,'rank_score':.9-i*.1}
        hits.append({'section':source,'inclusion':'seed','scores':scores})
        candidates.append({'key':{'source_id':source['source_id'],'section_id':source['id']},'scores':scores})
    outcome=record['outcome']
    outcome.update(query=case['question'],hits=hits,seeds=[c['key'] for c in candidates],candidates=candidates,omitted=[])
    outcome['work']['union_candidates']=len(candidates)
    outcome['estimated_context_tokens']=sum((len(json.dumps({k:h['section'][k] for k in ('source_id','source_date','source_title','id','heading','text')},ensure_ascii=False,separators=(',',':')))+3)//4 for h in hits)
    return {'schema_version':1,'operation':'retrieve','status':'completed','dataset_id':'legacy','case_id':case_id,'question':case['question'],'record':record}


def codex_record(payload_mode='compact', count=1):
    local=local_record();native=local['record'];outcome=native['outcome']
    sources=[]
    for name in ('document.md','bicycle-workshop.md','language-cafe.md','community-cooking.md','walking.md'):
        item=section(name,'S1');sources.append({'id':item['source_id'],'title':item['source_title'],'date':item['source_date']})
    full={'schema_version':1,'question':local['question'],'top_k':3,'as_of':'2026-10-20','sources':sources,'manifest':{'config':native['config'],'corpus_identity':outcome['corpus_identity']},'outcome':outcome,'hits':outcome['hits'],'preparation':native['preparation']}
    encode=lambda value:json.dumps(value,sort_keys=True,ensure_ascii=False,separators=(',',':'))
    audits=[];calls=[];session='unit-mcp-audit'
    for index in range(1,count+1):
        digest=hashlib.sha256(encode(full).encode()).hexdigest()
        trace={'session_id':session,'call_index':index,'request_id':f'{session}-request-{index}','trace_id':f'{session}-trace-{index}','full_response_sha256':digest}
        visible=full if payload_mode=='full' else {**{key:full[key] for key in ('question','top_k','sources','as_of','manifest')},'schema_version':2,'payload_mode':'compact','hits':[{'section':hit['section']} for hit in full['hits']],'trace':trace}
        audits.append({'schema_version':1,**trace,'payload_mode':payload_mode,'payload_bytes':len(encode(visible).encode()),'full_response':full})
        calls.append({'server':'coa','tool':'search_document','query':local['question'],'top_k':3,'status':'completed','result':copy.deepcopy(visible)})
    raw=''.join(encode(row)+'\n' for row in audits)
    live={'schema_version':2,'run_id':'native-codex-one','status':'completed','question':local['question'],'config':native['config'],'filters':native['request_filters'],'source_guard_passed':True,'payload_mode':payload_mode,'trace_integrity_status':'validated','source_audit_jsonl':raw,'audit_trace':{'session_id':session,'sha256':hashlib.sha256(raw.encode()).hexdigest(),'trace_bytes':len(raw.encode()),'payload_bytes':sum(row['payload_bytes'] for row in audits),'successful_searches':len(audits)},'run':{'status':'completed','question':local['question'],'model':'gpt-6.1-sol','sources':sources,'as_of':'2026-10-20','tool_calls':calls}}
    return {**local,'operation':'codex','record':live}


class AssertionsTest(unittest.TestCase):
    def setUp(self):
        self.assertions = load_assertions()
        self.assertIsNotNone(self.assertions, 'Canonical promptfoo assertions are not implemented')

    def grade(self, result, extra=None):
        context={'vars': {'dataset':result['dataset_id'],'case_id':result['case_id'],'question':result['question'],'method':'dense','profile':'documents_sections','top_k':3,'selection':'sections','candidate_k':30,'context_token_limit':1200,'apply_filters':False,'mmr':False,'rrf_k':60.0,'vector_weight':.7,'mmr_lambda':.7,'model':'gpt-6.1-sol'}}
        context['vars'].update(extra or {})
        return self.assertions.get_assert(json.dumps(result),context)

    def test_complete_required_sources_pass_with_separate_completeness_and_coverage(self):
        grade = self.grade(local_record())
        self.assertTrue(grade['pass'])
        self.assertEqual(grade['namedScores']['source_complete'], 1)
        self.assertEqual(grade['namedScores']['goal_coverage'], 1)

    def test_missing_source_is_real_assertion_failure_despite_completed_execution(self):
        grade = self.grade(local_record(('S2',)))
        self.assertFalse(grade['pass'])
        self.assertEqual(grade['namedScores']['source_complete'], 0)
        self.assertEqual(grade['namedScores']['goal_coverage'], .5)
        self.assertEqual(grade['namedScores']['execution_integrity'], 1)

    def test_no_positive_target_does_not_receive_perfect_quality_score(self):
        grade = self.grade(local_record((), 'P05'))
        self.assertNotIn('source_complete', grade['namedScores'])
        self.assertNotIn('goal_coverage', grade['namedScores'])
        self.assertIn('not applicable', grade['reason'].lower())

    def test_mutated_source_text_or_date_fails_integrity(self):
        for field in ('text','source_date'):
            result = local_record(); result['record']['outcome']['hits'][0]['section'][field] += ' changed'
            grade = self.grade(result)
            self.assertFalse(grade['pass'])
            self.assertEqual(grade['namedScores']['execution_integrity'], 0)
            self.assertNotIn('source_complete', grade['namedScores'])

    def test_failed_unsupported_or_missing_outcome_never_becomes_quality_pass(self):
        for state in ('failed','unsupported'):
            result=local_record();result['record']['execution_status']=state;result['record']['outcome']=None
            grade=self.grade(result)
            self.assertFalse(grade['pass']);self.assertNotIn('source_complete',grade['namedScores'])

    def test_mismatched_question_or_case_is_rejected(self):
        result=local_record();result['record']['question']='Different query'
        self.assertFalse(self.grade(result)['pass'])

    def test_and_or_evidence_alternatives_cannot_be_mixed(self):
        goals=[{'evidence_alternatives': [[{'source_id':'A','section_id':'S1'},{'source_id':'A','section_id':'S2'}],[{'source_id':'B','section_id':'S1'},{'source_id':'B','section_id':'S2'}]]}]
        metrics=self.assertions.goal_metrics(goals,{('A','S1'),('B','S2')})
        self.assertEqual(metrics['goal_coverage'],0)
        self.assertFalse(metrics['source_complete'])

    def test_goal_logic_retains_and_or_empty_overlap_and_disjoint_cases(self):
        ref=lambda source,number:{'source_id':source,'section_id':number}
        goals=[{'evidence_alternatives':[[ref('A','S1'),ref('A','S2')],[ref('B','S1'),ref('B','S2')]]},{'evidence_alternatives':[[ref('B','S2')],[ref('C','S1')]]}]
        for offered,expected in [(set(),0),({('A','S1'),('B','S2')},.5),({('A','S1'),('A','S2'),('C','S1')},1),({('B','S1'),('B','S2')},1),({('Z','S1')},0)]:
            metric=self.assertions.goal_metrics(goals,offered)
            self.assertEqual(metric['goal_coverage'],expected);self.assertEqual(metric['source_complete'],expected==1)
            self.assertEqual(metric['minimum_required_sections'],2)

    def test_minimum_union_counts_shared_sections_once(self):
        goals=[{'evidence_alternatives':[[{'source_id':'A','section_id':'S1'}]]},{'evidence_alternatives':[[{'source_id':'A','section_id':'S1'},{'source_id':'A','section_id':'S2'}]]}]
        self.assertEqual(self.assertions.goal_metrics(goals,set())['minimum_required_sections'],2)

    def test_extra_unselected_canonical_hit_cannot_complete_sections_result(self):
        result=local_record(('S2',))
        extra={'section':section('document.md','S3'),'inclusion':'activity_expansion','scores':None}
        result['record']['outcome']['hits'].append(extra)
        result['record']['outcome']['estimated_context_tokens']+=100
        grade=self.grade(result)
        self.assertFalse(grade['pass'])
        self.assertEqual(grade['namedScores']['execution_integrity'],0)

    def test_wrong_corpus_or_method_or_profile_cannot_pass(self):
        changes=[lambda r:r['dataset'].update(corpus_sha256='0'*64),lambda r:r['config']['retrieval'].update(method='bm25'),lambda r:r['config'].update(profile='sqlite_sections')]
        for mutate in changes:
            result=local_record();mutate(result['record'])
            grade=self.grade(result)
            self.assertFalse(grade['pass']);self.assertEqual(grade['namedScores']['execution_integrity'],0)

    def test_inconsistent_candidate_or_omission_trace_is_rejected(self):
        result=local_record();result['record']['outcome']['candidates']=[]
        self.assertFalse(self.grade(result)['pass'])

    def test_failed_envelope_with_completed_record_is_not_a_pass(self):
        result=local_record();result['status']='failed'
        self.assertFalse(self.grade(result)['pass'])

    def test_codex_sources_bind_actual_model_config_query_and_validated_trace(self):
        result=codex_record()
        self.assertTrue(self.grade(result)['pass'])
        mutations=[lambda r:r['run'].update(model='gpt-6-luna'),lambda r:r.update(trace_integrity_status='invalid'),lambda r:r['run'].update(question='unrelated'),lambda r:r['config'].update(profile='sqlite_sections'),lambda r:r['run']['tool_calls'][0].update(server='untrusted'),lambda r:r['run']['tool_calls'][0].update(tool='shell'),lambda r:r.update(payload_mode='full')]
        for mutate in mutations:
            changed=copy.deepcopy(result);mutate(changed['record'])
            self.assertFalse(self.grade(changed)['pass'])

    def test_full_and_compact_have_the_same_strict_source_quality_from_bound_audit(self):
        compact=self.grade(codex_record())
        full=self.grade(codex_record('full'),{'payload_mode':'full'})
        self.assertTrue(compact['pass']);self.assertTrue(full['pass'])
        self.assertEqual(compact['namedScores']['goal_coverage'],full['namedScores']['goal_coverage'])

    def test_missing_corrupted_or_reordered_audit_and_extra_visible_hits_are_rejected(self):
        result=codex_record(count=2)
        changes=[lambda r:r.pop('source_audit_jsonl'),lambda r:r.update(source_audit_jsonl=r['source_audit_jsonl']+' '),lambda r:r['run']['tool_calls'][0]['result']['hits'].append({'section':section('document.md','S4')})]
        for mutate in changes:
            changed=copy.deepcopy(result);mutate(changed['record'])
            self.assertFalse(self.grade(changed)['pass'])
        changed=copy.deepcopy(result);record=changed['record'];record['source_audit_jsonl']='\n'.join(reversed(record['source_audit_jsonl'].splitlines()))+'\n'
        record['audit_trace']['sha256']=hashlib.sha256(record['source_audit_jsonl'].encode()).hexdigest()
        self.assertFalse(self.grade(changed)['pass'])

    def test_multisearch_does_not_treat_per_call_topk_as_a_cumulative_capacity_limit(self):
        grade=self.grade(codex_record(count=2))
        metrics=grade['metadata']['source_metrics']
        self.assertIsNone(metrics.get('section_capacity_limit'))
        self.assertEqual(metrics.get('per_search_section_capacity_limit'),3)

    def test_activity_expansion_reports_the_minimum_activity_capacity_bound(self):
        result=local_record(('S5',),'T01');record=result['record']
        for config in (record['config']['retrieval'],record['outcome']['config']):config.update(selection='activity_expansion',top_k=1)
        variables={'dataset':'legacy','case_id':'T01','question':result['question'],'profile':'documents_sections',**record['config']['retrieval']}
        metric=self.assertions.measure(result,variables)
        self.assertEqual(metric.get('minimum_required_activities'),3)
        self.assertEqual(metric.get('seed_activity_capacity_limit'),1)
        self.assertTrue(metric['goal_set_exceeds_capacity'])

    def test_a_valid_empty_selection_has_known_zero_reciprocal_rank(self):
        result=local_record(())
        grade=self.grade(result)
        self.assertEqual(grade['namedScores'].get('first_goal_support_reciprocal_rank'),0)
        self.assertEqual(grade['namedScores'].get('first_relevant_reciprocal_rank'),0)

    def test_garden_word_overlap_is_named_separately_from_bm25(self):
        labels=self.assertions.load_case('garden','Q1')
        self.assertEqual(labels['facts'][0]['evidence_alternatives'][0][0],{'source_id':'ACT-TUIN-001','section_id':'S2'})
        self.assertEqual(len(labels['facts']),2)


if __name__=='__main__':
    unittest.main()
