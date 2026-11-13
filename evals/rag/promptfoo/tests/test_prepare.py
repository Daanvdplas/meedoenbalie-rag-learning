import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from test_assertions import HERE,local_record
from test_provider import load_provider

class PreparedExecutionTest(unittest.TestCase):
    def setUp(self):
        path=HERE/'prepare.py';self.assertTrue(path.exists(),'Fresh batch preparation is not implemented')
        spec=importlib.util.spec_from_file_location('coa_prepare_test',path);self.prepare=importlib.util.module_from_spec(spec);spec.loader.exec_module(self.prepare)
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name)
        binary=self.root/'runtime';binary.write_text('#!fake');binary.chmod(0o700)
        self.env={'COA_RAG_BINARY':str(binary),'COA_PROMPTFOO_ARTIFACT_DIR':str(self.root/'artifacts'),'COA_PROMPTFOO_EXECUTION_ID':'fresh-test-execution'}
        self.rows=[]
        for case in ('P01','P02'):
            native=local_record(case_id=case)
            self.rows.append({'vars':{**self.prepare.provider.DEFAULTS,'filters':{},'dataset':'legacy','case_id':case,'question':native['question']}})

    def test_exact_selected_cases_share_preparation_without_native_assessment(self):
        seen=[]
        def execute(command,env,timeout,cwd):
            seen.append(command)
            cases=command[command.index('--cases')+1].split(',')
            output=Path(command[command.index('--output')+1])
            output.write_text(''.join(json.dumps(local_record(case_id=case)['record'])+'\n' for case in cases))
            return subprocess.CompletedProcess(command,0,b'{}',b'')
        request={'schema_version':1,'execution_id':'fresh-test-execution','tests':self.rows}
        output=self.root/'prepared.json'
        with patch.dict(os.environ,self.env),patch.object(self.prepare.provider,'_execute',side_effect=execute):
            manifest=self.prepare.prepare(request,output)
        self.assertEqual(len(seen),1);self.assertEqual(set(seen[0][seen[0].index('--cases')+1].split(',')),{'P01','P02'})
        self.assertNotIn('assess',seen[0]);self.assertEqual(len(manifest['records']),2)
        provider=load_provider()
        with patch.dict(os.environ,{**self.env,'COA_PROMPTFOO_PREPARED':str(output)}),patch.object(provider,'_execute') as execute_again:
            result=provider.call_api('',{},self.rows[0])
        self.assertNotIn('error',result);execute_again.assert_not_called()
        self.assertEqual(result['metadata']['execution_mode'],'fresh_batch_retrieval')

    def test_native_independent_per_case_filters_are_not_overridden_with_a_global_flag(self):
        rows=[]
        for case,filters in [('IND26-01',{'environment':'indoor'}),('IND26-03',{'weekdays':['sunday']})]:
            rows.append({'vars':{**self.prepare.provider.DEFAULTS,'mode':'local','apply_filters':True,'selection':'activity_expansion','dataset':'independent-20261103','case_id':case,'question':self.prepare.provider._question('independent-20261103',case),'filters':filters}})
        observed=[]
        def execute(command,env,timeout,cwd):
            observed.append(command)
            self.assertNotIn('--query-filters',command)
            native=[]
            for row in rows:
                record=local_record()['record'];record.update(case_id=row['vars']['case_id'],question=row['vars']['question']);native.append(record)
            Path(command[command.index('--output')+1]).write_text(''.join(json.dumps(record)+'\n' for record in native))
            return subprocess.CompletedProcess(command,0,b'{}',b'')
        with patch.dict(os.environ,self.env),patch.object(self.prepare.provider,'_execute',side_effect=execute):
            self.prepare.prepare({'schema_version':1,'execution_id':'fresh-test-execution','tests':rows},self.root/'ind-prepared.json')
        self.assertEqual(len(observed),1)

    def test_changed_native_record_or_wrong_execution_id_is_not_a_cache_hit(self):
        request={'schema_version':1,'execution_id':'fresh-test-execution','tests':[]}
        output=self.root/'prepared.json'
        with patch.dict(os.environ,self.env):self.prepare.prepare(request,output)
        provider=load_provider()
        with patch.dict(os.environ,{**self.env,'COA_PROMPTFOO_PREPARED':str(output),'COA_PROMPTFOO_EXECUTION_ID':'another-execution'}),patch.object(provider,'_execute') as execute:
            result=provider.call_api('',{},self.rows[0])
        self.assertIn('error',result);self.assertEqual(execute.call_count,0)

if __name__=='__main__':unittest.main()
