import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from test_assertions import HERE,load_assertions
from test_provider import load_provider

class ModelProviderBoundaryTest(unittest.TestCase):
    def setUp(self):
        self.provider=load_provider();self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name)
        binary=self.root/'runtime';binary.write_text('#!fake');binary.chmod(0o700)
        self.ledger=self.root/'ledger.json'
        self.env={'COA_RAG_BINARY':str(binary),'COA_PROMPTFOO_ARTIFACT_DIR':str(self.root/'artifacts'),'COA_PROMPTFOO_ALLOW_CODEX':'1','COA_PROMPTFOO_LEDGER':str(self.ledger),'COA_PROMPTFOO_MAX_GENERATIONS':'8','COA_PROMPTFOO_MAX_SECONDS':'900','COA_PROMPTFOO_CASES':'RAG26-01','COA_PROMPTFOO_REPEATS':'1'}
        self.variables={**self.provider.DEFAULTS,'mode':'codex','method':'dense','dataset':'strengthened-10-20261106','case_id':'RAG26-01','question':self.provider._question('strengthened-10-20261106','RAG26-01'),'slot':1,'trial':1,'phase':'smoke'}
        self.context={'vars':self.variables,'test':{'metadata':{'suite':'models'}}}
        self.raw={'schema_version':1,'dataset_id':self.variables['dataset'],'case_id':self.variables['case_id'],'requested_model':'gpt-6.1-sol','dataset':{'corpus_sha256':json.loads((HERE.parents[2]/'evals/shared/datasets/strengthened-10-20261106/questions.json').read_text())['corpus_sha256']},'live':{'schema_version':2,'status':'failed','question':self.variables['question'],'run_id':'verified-failed-model','run':None,'error':'model_contract_violation','safe_diagnostic':{'category':'model_contract_violation','stage':'agent_event_validation','exit_code':0,'json_event_lines_valid':True,'event_types':['turn.completed'],'item_types':['agent_message'],'provider_error_code':None,'validation_reason':'missing_required_search','tool_event_category':'none'},'audit_trace':None,'trace_integrity_status':'not_checked','source_guard_passed':None}}

    def execute(self,command,env,timeout,cwd):
        Path(command[command.index('--output')+1]).write_text(json.dumps(self.raw)+'\n')
        return subprocess.CompletedProcess(command,1,b'{}',b'private output must not be exported')

    def test_models_cannot_start_without_an_explicit_protocol_phase(self):
        env={**self.env,'COA_PROMPTFOO_PHASE':'plan'}
        with patch.dict(os.environ,env),patch.object(self.provider,'_execute') as execute:
            result=self.provider.call_api('',{},self.context)
        self.assertIn('error',result);self.assertEqual(execute.call_count,0);self.assertFalse(self.ledger.exists())

    def test_closed_live_gate_does_not_reserve_model_protocol_state(self):
        with patch.dict(os.environ,{**self.env,'COA_PROMPTFOO_PHASE':'pilot','COA_PROMPTFOO_ALLOW_CODEX':'0'}),patch.object(self.provider,'_model_controls') as controls,patch.object(self.provider,'_execute') as execute:
            result=self.provider.call_api('',{},self.context)
        self.assertIn('error',result)
        self.assertEqual(controls.call_count,0)
        self.assertEqual(execute.call_count,0)
        self.assertFalse(self.ledger.exists())

    def test_verified_model_defect_consumes_attempt_but_keeps_source_quality_unknown(self):
        with patch.dict(os.environ,{**self.env,'COA_PROMPTFOO_PHASE':'smoke'}),patch.object(self.provider,'_execute',side_effect=self.execute):
            result=self.provider.call_api('',{},self.context)
        self.assertNotIn('error',result)
        state=json.loads(self.ledger.read_text());self.assertFalse(state['stopped']);self.assertEqual(state['attempts'][0]['status'],'model_contract_violation')
        assertions=load_assertions();self.assertTrue(callable(getattr(assertions,'get_model_assert',None)))
        grade=assertions.get_model_assert(result['output'],self.context)
        self.assertFalse(grade['pass']);self.assertNotIn('source_complete',grade['namedScores'])
        self.assertEqual(grade['metadata']['source_metrics']['model_failure_class'],'model_contract_violation')
        self.assertTrue(grade['metadata']['source_metrics']['execution_integrity'])

    def test_authentication_or_trace_errors_do_not_become_verified_model_outcomes(self):
        self.raw['live']['safe_diagnostic']['provider_error_code']='invalid_api_key'
        with patch.dict(os.environ,{**self.env,'COA_PROMPTFOO_PHASE':'smoke'}),patch.object(self.provider,'_execute',side_effect=self.execute):
            result=self.provider.call_api('',{},self.context)
        self.assertIn('error',result);self.assertTrue(json.loads(self.ledger.read_text())['stopped'])

if __name__=='__main__':unittest.main()
