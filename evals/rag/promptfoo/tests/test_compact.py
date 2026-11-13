import copy
import hashlib
import json
import unittest
from test_assertions import load_assertions,local_record,section


def compact_result():
    local=local_record();native=local['record'];outcome=native['outcome']
    sources=[]
    for name in ('document.md','bicycle-workshop.md','language-cafe.md','community-cooking.md','walking.md'):
        item=section(name,'S1');sources.append({'id':item['source_id'],'title':item['source_title'],'date':item['source_date']})
    full={'question':local['question'],'top_k':3,'as_of':'2026-10-20','sources':sources,'manifest':{'config':native['config'],'corpus_identity':outcome['corpus_identity']},'outcome':outcome,'hits':outcome['hits'],'preparation':native['preparation']}
    full_text=json.dumps(full,ensure_ascii=False,separators=(',',':'),sort_keys=True)
    compact={key:full[key] for key in ('question','top_k','as_of','sources','manifest')}
    compact.update(schema_version=2,payload_mode='compact',hits=[{'section':h['section']} for h in full['hits']],trace={'full_response_sha256':hashlib.sha256(full_text.encode()).hexdigest()})
    compact_text=json.dumps(compact,ensure_ascii=False,separators=(',',':'),sort_keys=True)
    result={**local,'operation':'compact','record':{'schema_version':1,'status':'completed','execution_mode':'offline_projection_replay','input_sha256':'a'*64,'full_response':full,'compact_response':compact,'full_response_json':full_text,'compact_response_json':compact_text,'full_payload_bytes':len(full_text.encode()),'compact_payload_bytes':len(compact_text.encode())}}
    variables={**native['config']['retrieval'],'profile':native['config']['profile'],'dataset':'legacy','case_id':'P01','question':local['question'],'input_sha256':'a'*64}
    return result,variables

class CompactTest(unittest.TestCase):
    def test_projection_preserves_exact_sources_and_reports_only_payload_metrics(self):
        result,variables=compact_result();grade=load_assertions().get_assert(json.dumps(result),{'vars':variables})
        self.assertTrue(grade['pass']);self.assertNotIn('source_complete',grade['namedScores'])
        self.assertGreater(grade['namedScores']['payload_reduction'],0)
        self.assertEqual(grade['metadata']['source_metrics']['answer_quality'],'not_assessed')

    def test_mutated_projection_or_wrong_input_digest_is_rejected(self):
        result,variables=compact_result()
        for mutate in [lambda x:x['compact_response']['hits'][0]['section'].update(source_date='2026-01-01'),lambda x:x.update(input_sha256='b'*64),lambda x:x.update(full_payload_bytes=1)]:
            changed=copy.deepcopy(result);mutate(changed['record'])
            self.assertFalse(load_assertions().get_assert(json.dumps(changed),{'vars':variables})['pass'])

if __name__=='__main__':unittest.main()
