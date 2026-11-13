import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from test_assertions import HERE,codex_record,load_assertions

class RegradeAuditTest(unittest.TestCase):
    def setUp(self):
        file=HERE/'regrade_inputs.py';self.assertTrue(file.exists(),'Strict native audit enrichment is not implemented')
        spec=importlib.util.spec_from_file_location('coa_regrade_inputs_test',file);self.module=importlib.util.module_from_spec(spec);spec.loader.exec_module(self.module)
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name)
        envelope=codex_record();live=envelope['record'];raw=live.pop('source_audit_jsonl')
        native=json.loads(json.dumps(live));trace=self.root/'native.mcp-trace.jsonl';trace.write_text(raw)
        native['audit_trace']['path']=str(trace)
        (self.root/'native.jsonl').write_text(json.dumps({'live':native})+'\n')
        self.input=self.root/'original.json';self.output=self.root/'enriched.json'
        self.input.write_text(json.dumps({'evalId':'original-eval','results':{'results':[{'vars':{'dataset':'legacy','case_id':'P01','question':envelope['question'],'profile':'documents_sections',**live['config']['retrieval'],'model':'gpt-6.1-sol'},'response':{'output':envelope}}]}}))

    def test_enrichment_binds_original_native_audit_without_modifying_old_evidence(self):
        before=self.input.read_bytes();result=self.module.enrich(self.input,self.output)
        self.assertEqual(self.input.read_bytes(),before)
        row=result['results']['results'][0];envelope=row['response']['output']
        self.assertIn('source_audit_jsonl',envelope['record'])
        self.assertNotIn('path',envelope['record']['audit_trace'])
        self.assertTrue(load_assertions().get_assert(envelope,{'vars':row['vars']})['pass'])
        self.assertEqual(result['metadata']['source_audit_enrichment']['original_raw_sha256'],hashlib.sha256(before).hexdigest())

    def test_corrupted_audit_is_rejected_without_a_new_output(self):
        with (self.root/'native.mcp-trace.jsonl').open('a') as handle:handle.write(' ')
        with self.assertRaises(ValueError):self.module.enrich(self.input,self.output)
        self.assertFalse(self.output.exists())

if __name__=='__main__':unittest.main()
