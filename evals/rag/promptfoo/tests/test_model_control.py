import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

HERE=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('model_control',HERE/'model_control.py')
control=importlib.util.module_from_spec(spec);spec.loader.exec_module(control)

class ModelProtocolTests(unittest.TestCase):
    def test_complete_schedule_is_exactly_the_historical_balanced_order(self):
        rows=control.ordered_schedule()
        expected=control.legacy.schedule(control.case_ids())
        self.assertEqual([{k:row[k] for k in expected[0]} for row in rows],expected)
        self.assertEqual(len(rows),60)
        self.assertEqual([row['phase'] for row in rows[:4]],['pilot']*4)
        self.assertEqual([row['model'] for row in rows[:4]],['gpt-6.1-sol','gpt-6-luna','gpt-6-luna','gpt-6.1-sol'])

    def setup_protocol(self, directory):
        binary=Path(directory)/'runtime';binary.write_bytes(b'pinned test runtime')
        codex=Path(directory)/'codex';codex.write_bytes(b'pinned fake Codex')
        (Path(directory)/'codex-code-mode-host').write_bytes(b'pinned fake host')
        return patch.dict(os.environ,{'COA_CODEX_BIN':str(codex),'COA_RAG_BINARY':str(binary),'COA_PROMPTFOO_LEDGER':str(Path(directory)/'ledger.json'),'COA_PROMPTFOO_PHASE':'pilot','COA_PROMPTFOO_MAX_GENERATIONS':'60','COA_PROMPTFOO_MAX_SECONDS':'7200','COA_PROMPTFOO_SUITE':'models'},clear=False)

    def test_pilot_cannot_skip_or_retry_a_reserved_slot(self):
        with tempfile.TemporaryDirectory() as directory,self.setup_protocol(directory):
            row=control.ordered_schedule()[0]
            with self.assertRaisesRegex(ValueError,'order|slot'):
                control.authorize_model({**row,'case_id':'RAG26-10'})
            token=control.authorize_model(row)
            with self.assertRaisesRegex(ValueError,'Uncertain|reserved|failed'):
                control.authorize_model(row)
            self.assertEqual(control.finish_model(token,None,1,None)['status'],'failed')
            with self.assertRaisesRegex(ValueError,'stopped|failed|Uncertain'):
                control.authorize_model(row)

    def test_pilot_approval_is_bound_to_exact_attempt_bytes_and_preregistration(self):
        attempts=[{**row,'status':'completed','record_sha256':str(i)} for i,row in enumerate(control.legacy.schedule(control.case_ids())[:4])]
        approval={'approved':True,'reviewer':'independent','preregistration_sha256':'prereg','pilot_attempts_sha256':control.legacy.json_hash(attempts)}
        control.require_approval(approval,'prereg',attempts)
        with self.assertRaises(ValueError):control.require_approval(approval,'different',attempts)
        with self.assertRaises(ValueError):control.require_approval(approval,'prereg',[{**attempts[0],'record_sha256':'changed'},*attempts[1:]])
        with self.assertRaises(ValueError):control.require_approval({**approval,'reviewer':''},'prereg',attempts)

    def test_smoke_is_explicit_small_and_does_not_create_a_fake_pilot(self):
        with tempfile.TemporaryDirectory() as directory,self.setup_protocol(directory),patch.dict(os.environ,{'COA_PROMPTFOO_PHASE':'smoke','COA_PROMPTFOO_CASES':'RAG26-01','COA_PROMPTFOO_REPEATS':'1','COA_PROMPTFOO_MAX_GENERATIONS':'8','COA_PROMPTFOO_MAX_SECONDS':'900'}):
            token=control.authorize_model({'case_id':'RAG26-01','model':'gpt-6.1-sol'})
            self.assertTrue(token['smoke'])
            self.assertFalse((Path(directory)/'ledger.models-state.json').exists())
            with patch.dict(os.environ,{'COA_PROMPTFOO_REPEATS':'3'}):
                with self.assertRaises(ValueError):control.authorize_model({'case_id':'RAG26-01','model':'gpt-6.1-sol'})

    def test_four_pilot_attempts_require_hash_bound_approval_before_remaining(self):
        with tempfile.TemporaryDirectory() as directory,self.setup_protocol(directory):
            root=Path(directory);native=root/'native.jsonl';native.write_text('native evidence')
            audit=root/'audit.jsonl';audit.write_text('audit evidence')
            with patch.object(control,'_bound_native',return_value=(native,audit)),patch.object(control,'classify_result',return_value='completed'),patch.object(control,'_bind_common'):
                for row in control.ordered_schedule()[:4]:
                    token=control.authorize_model(row)
                    self.assertEqual(control.finish_model(token,{},0,native)['status'],'completed')
            with self.assertRaisesRegex(ValueError,'Pilot ends'):
                control.authorize_model(control.ordered_schedule()[4])
            with patch.dict(os.environ,{'COA_PROMPTFOO_PHASE':'remaining'}):
                with self.assertRaises(FileNotFoundError):control.authorize_model(control.ordered_schedule()[4])
                state=json.loads((root/'ledger.models-state.json').read_text())
                approval={'approved':True,'reviewer':'independent-review','preregistration_sha256':state['preregistration_sha256'],'pilot_attempts_sha256':control.legacy.json_hash(state['attempts'])}
                (root/'PILOT_APPROVED.json').write_text(json.dumps(approval))
                token=control.authorize_model(control.ordered_schedule()[4])
                self.assertEqual(token['slot'],5)
                with patch.object(control,'_bound_native',return_value=(native,audit)),patch.object(control,'classify_result',return_value='completed'),patch.object(control,'_bind_common'):
                    control.finish_model(token,{},0,native)
                self.assertEqual(control.execution_schedule()[0]['slot'],6)
                token=control.authorize_model(control.ordered_schedule()[5])
                self.assertEqual(control.finish_model(token,None,1,None)['status'],'failed')
                with self.assertRaises(ValueError):control.execution_schedule()

    def test_pinned_runtime_change_and_native_evidence_change_reject_resume(self):
        with tempfile.TemporaryDirectory() as directory,self.setup_protocol(directory):
            row=control.ordered_schedule()[0];token=control.authorize_model(row)
            native=Path(directory)/'native.jsonl';native.write_text('native')
            audit=Path(directory)/'audit.jsonl';audit.write_text('audit')
            with patch.object(control,'_bound_native',return_value=(native,audit)),patch.object(control,'classify_result',return_value='completed'),patch.object(control,'_bind_common'):
                control.finish_model(token,{},0,native)
            native.write_text('changed')
            with self.assertRaisesRegex(ValueError,'evidence changed'):control.authorize_model(control.ordered_schedule()[1])
            native.write_text('native');(Path(directory)/'runtime').write_bytes(b'changed runtime')
            with self.assertRaisesRegex(ValueError,'Pinned'):control.authorize_model(control.ordered_schedule()[1])

    def test_both_models_share_actual_codex_host_and_full_embedding_vector_identity(self):
        pins={'codex':'codexhash','code_mode_host':'hosthash','mcp':'runtimehash'}
        identity={'source_sha256':'source','embedding':{'model':'minilm','dimensions':384},'embedding_inputs_sha256':'inputs','vectors_sha256':'vectors'}
        def record(model):
            return {'requested_model':model,'live':{'runtime_configuration':{'model':model,'binaries':dict(pins),'reasoning_effort':'low'},'run':{'manifest':{'corpus_identity':json.loads(json.dumps(identity))}}}}
        state={};control._bind_common(state,record('gpt-6.1-sol'),{'runtime_binaries':pins},None)
        control._bind_common(state,record('gpt-6-luna'),{'runtime_binaries':pins},None)
        changed=record('gpt-6-luna');changed['live']['runtime_configuration']['binaries']['code_mode_host']='changed'
        with self.assertRaisesRegex(ValueError,'runtime'):control._bind_common(state,changed,{'runtime_binaries':pins},None)
        for field in ['vectors_sha256','embedding_inputs_sha256','embedding']:
            changed=record('gpt-6-luna');changed['live']['run']['manifest']['corpus_identity'][field]='changed'
            with self.assertRaisesRegex(ValueError,'embedding/vector'):control._bind_common(state,changed,{'runtime_binaries':pins},None)

    def test_proven_no_search_model_contract_failure_may_have_no_audit_but_corruption_never_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'native.jsonl';path.write_text('native')
            case=control.case_ids()[0];question=json.loads((control.DATA/'questions.json').read_text())['cases'][0]['question']
            corpus=json.loads((control.DATA/'FROZEN.json').read_text())['integrity']['corpus_sha256']
            record={'dataset_id':control.DATASET,'dataset':{'corpus_sha256':corpus},'case_id':case,'requested_model':'gpt-6.1-sol','live':{'question':question,'run':None,'safe_diagnostic':{'validation_reason':'missing_required_search'}}}
            token={'case_id':case,'model':'gpt-6.1-sol'}
            self.assertEqual(control._bound_native(token,record,path,'model_contract_violation'),(path.resolve(),None))
            record['live']['trace_integrity_status']='invalid'
            with self.assertRaises(ValueError):control._bound_native(token,record,path,'model_contract_violation')

    def test_clean_partial_pilot_schedule_resumes_after_the_saved_prefix(self):
        with tempfile.TemporaryDirectory() as directory,self.setup_protocol(directory):
            self.assertEqual([row['slot'] for row in control.execution_schedule()],list(range(1,5)))
            row=control.ordered_schedule()[0];token=control.authorize_model(row)
            native=Path(directory)/'native.jsonl';native.write_text('native')
            audit=Path(directory)/'audit.jsonl';audit.write_text('audit')
            with self.assertRaisesRegex(ValueError,'Uncertain|reserved'):control.execution_schedule()
            with patch.object(control,'_bound_native',return_value=(native,audit)),patch.object(control,'classify_result',return_value='completed'),patch.object(control,'_bind_common'):
                control.finish_model(token,{},0,native)
            self.assertEqual([row['slot'] for row in control.execution_schedule()],[2,3,4])

    def test_network_or_unknown_failure_is_never_a_model_result(self):
        self.assertEqual(control.classify_result(None,1),'failed')
        record={'live':{'status':'failed','error':'network_error','run':None,'safe_diagnostic':{}}}
        self.assertEqual(control.classify_result(record,1),'failed')
        record['live'].update(error='model_output_invalid',safe_diagnostic={'category':'model_output_invalid','stage':'agent_event_validation','exit_code':0,'json_event_lines_valid':True,'event_types':['turn.completed'],'item_types':['agent_message'],'validation_reason':'invalid_model_answer'})
        self.assertEqual(control.classify_result(record,1),'model_output_invalid')
        record['live']['safe_diagnostic']['event_types'].append('turn.failed')
        self.assertEqual(control.classify_result(record,1),'failed')

if __name__=='__main__':unittest.main()
