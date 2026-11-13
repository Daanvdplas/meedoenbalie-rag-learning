import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from test_provider import load_provider
from test_assertions import local_record

class BudgetTest(unittest.TestCase):
    def setUp(self):
        self.provider=load_provider();self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name);self.ledger=self.root/'ledger.json';binary=self.root/'runtime';binary.write_text('#!fake');binary.chmod(0o700)
        q=local_record()['question'];self.context={'vars':{'dataset':'legacy','case_id':'P01','question':q}}
        self.env={'COA_RAG_BINARY':str(binary),'COA_PROMPTFOO_ARTIFACT_DIR':str(self.root/'artifacts'),'COA_PROMPTFOO_ALLOW_CODEX':'1','COA_PROMPTFOO_LEDGER':str(self.ledger),'COA_PROMPTFOO_MAX_GENERATIONS':'8','COA_PROMPTFOO_MAX_SECONDS':'900'}

    def test_process_timeout_is_bounded_by_remaining_shared_window(self):
        self.ledger.write_text(json.dumps({'schema_version':1,'started_at':100,'max_generations':8,'max_seconds':900,'attempts':[],'stopped':False}))
        observed=[]
        def run(command,env,timeout,cwd):
            observed.append(timeout)
            Path(command[command.index('--output')+1]).write_text(json.dumps({'live':{'status':'completed','question':self.context['vars']['question'],'run_id':'native-one','run':{'model':'gpt-6.1-sol'}}})+'\n')
            return subprocess.CompletedProcess(command,0,b'{}',b'')
        with patch.dict(os.environ,self.env),patch.object(self.provider.time,'time',return_value=980),patch.object(self.provider,'_execute',side_effect=run):
            response=self.provider.call_api('',{'config':{'mode':'codex'}},self.context)
        self.assertNotIn('error',response)
        self.assertGreater(len(observed),0);self.assertLessEqual(observed[0],20)

    def test_failed_attempt_consumes_budget_and_stops_future_calls_without_retry(self):
        with patch.dict(os.environ,self.env),patch.object(self.provider,'_execute',return_value=subprocess.CompletedProcess([],1,b'',b'private')) as execute:
            first=self.provider.call_api('',{'config':{'mode':'codex'}},self.context)
            second=self.provider.call_api('',{'config':{'mode':'codex'}},self.context)
        self.assertIn('error',first);self.assertIn('error',second);self.assertEqual(execute.call_count,1)
        state=json.loads(self.ledger.read_text());self.assertEqual(len(state['attempts']),1);self.assertTrue(state['stopped'])

    def test_session_cap_is_shared_across_cases_and_cannot_be_reset_by_changing_limits(self):
        with patch.dict(os.environ,self.env):
            for i in range(8):
                path,attempt,*_=self.provider._reserve('legacy',f'case-{i}','gpt-6.1-sol')
                self.provider._finish(path,attempt,True)
            with self.assertRaises(ValueError):self.provider._reserve('legacy','ninth','gpt-6-luna')
        self.assertEqual(len(json.loads(self.ledger.read_text())['attempts']),8)
        with patch.dict(os.environ,{**self.env,'COA_PROMPTFOO_MAX_GENERATIONS':'9'}):
            with self.assertRaises(ValueError):self.provider._reserve('legacy','ninth','gpt-6.1-sol')

if __name__=='__main__':unittest.main()
