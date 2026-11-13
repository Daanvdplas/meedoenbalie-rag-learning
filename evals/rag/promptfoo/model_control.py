"""Retain the existing sixty-session model protocol around promptfoo execution.

This module schedules and reserves attempts; it never starts a model or grades
retrieval. Historical scheduling/failure predicates remain the contract.
"""
import argparse
import fcntl
import importlib.util
import json
import os
import shutil
from pathlib import Path
import time

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
spec=importlib.util.spec_from_file_location('historical_model_protocol',ROOT/'evals/rag/scripts/model_trials.py')
legacy=importlib.util.module_from_spec(spec);spec.loader.exec_module(legacy)
DATASET='strengthened-10-20261106'
DATA=ROOT/'evals/shared/datasets'/DATASET


def case_ids():
    return [row['id'] for row in json.loads((DATA/'questions.json').read_text())['cases']]


def ordered_schedule():
    return [{**row,'phase':'pilot' if row['slot']<=4 else 'remaining'} for row in legacy.schedule(case_ids())]


def require_approval(approval, preregistration_hash, attempts):
    if not isinstance(approval.get('reviewer'),str) or not approval['reviewer'].strip():
        raise ValueError('Independent reviewer identity is required')
    legacy.validate_pilot_approval(approval,preregistration_hash,attempts)


def classify_result(record, returncode):
    if not isinstance(record,dict) or not isinstance(record.get('live'),dict):return 'failed'
    live=record['live']
    if returncode==0 and live.get('status')=='completed' and live.get('source_guard_passed') is True and live.get('trace_integrity_status')=='validated' and isinstance(live.get('run'),dict) and live['run'].get('status')=='completed':
        return 'completed'
    try:
        if legacy.verified_model_failure(record,returncode):return live['error']
    except (KeyError,TypeError):pass
    return 'failed'


def _paths():
    ledger=Path(os.environ['COA_PROMPTFOO_LEDGER']).resolve()
    return ledger.with_suffix('.models-prereg.json'),ledger.with_suffix('.models-state.json'),ledger.with_suffix('.models.lock')


def _codex_binaries():
    configured=os.environ.get('COA_CODEX_BIN')
    if configured:codex=Path(configured).resolve(strict=True)
    else:
        home=Path(os.environ.get('CODEX_HOME',str(Path.home()/'.codex')))
        releases=home/'packages/app-server-daemon/releases'
        candidates=[path/'bin/codex' for path in releases.glob('*') if (path/'bin/codex').is_file() and (path/'bin/codex-code-mode-host').is_file()]
        codex=max(candidates,key=lambda path:path.parent.parent.stat().st_mtime) if candidates else Path(shutil.which('codex') or '').resolve(strict=True)
    host=codex.with_name('codex-code-mode-host').resolve(strict=True)
    return codex,host


def _pin_files():
    codex,host=_codex_binaries()
    files=[codex,host,Path(os.environ['COA_RAG_BINARY']).resolve(strict=True),HERE/'model_control.py',HERE/'provider.py',HERE/'assertions.py',HERE/'cases.cjs',HERE/'data-pins.json',DATA/'FROZEN.json',DATA/'questions.json']
    freeze=json.loads((DATA/'FROZEN.json').read_text())
    files += [ROOT/row['path'] for row in freeze['integrity']['sources']]
    return {str(path):legacy.file_hash(path) for path in files}


def _load_checked(prereg_path, state_path):
    prereg=legacy.load(prereg_path);state=legacy.load(state_path)
    if state['preregistration_sha256']!=legacy.file_hash(prereg_path) or prereg['schedule']!=legacy.schedule(case_ids()):
        raise ValueError('Model preregistration or schedule changed')
    for name,digest in prereg['pins'].items():
        if legacy.file_hash(Path(name))!=digest:raise ValueError('Pinned model protocol input/runtime changed')
    for attempt in state['attempts']:
        if attempt['status'] in ('completed',*legacy.MODEL_FAILURES):
            if legacy.file_hash(Path(attempt['record_path']))!=attempt['record_sha256']:raise ValueError('Previously saved native evidence changed')
            if attempt.get('audit_path') and legacy.file_hash(Path(attempt['audit_path']))!=attempt['audit_sha256']:raise ValueError('Previously saved audit evidence changed')
    return prereg,state


def authorize_model(variables):
    phase=os.environ.get('COA_PROMPTFOO_PHASE')
    if phase not in ('pilot','remaining','smoke'):raise ValueError('Models require an explicit pilot, remaining or smoke phase')
    if variables.get('dataset',DATASET)!=DATASET or variables.get('case_id') not in case_ids() or variables.get('model') not in legacy.MODELS:
        raise ValueError('Model protocol requires the canonical dataset/case/model')
    defaults={'payload_mode':'compact','method':'dense','profile':'documents_sections','top_k':3,'candidate_k':30,'context_token_limit':1200,'selection':'sections','apply_filters':False,'mmr':False,'rrf_k':60,'vector_weight':0.7,'mmr_lambda':0.7}
    if any(variables.get(key,value)!=value for key,value in defaults.items()) or variables.get('filters',{}):raise ValueError('Model comparison treatment differs from its fixed dense protocol')
    maximum=int(os.environ['COA_PROMPTFOO_MAX_GENERATIONS']);seconds=int(os.environ['COA_PROMPTFOO_MAX_SECONDS'])
    if phase=='smoke':
        selected=os.environ.get('COA_PROMPTFOO_CASES','').split(',')
        if not selected or not all(value in case_ids() for value in selected) or variables['case_id'] not in selected or os.environ.get('COA_PROMPTFOO_REPEATS')!='1' or not 1<=maximum<=8 or not 1<=seconds<=900:
            raise ValueError('Smoke requires selected canonical cases, repeat1 and at most8 sessions/900seconds')
        return {'smoke':True,'case_id':variables['case_id'],'model':variables['model']}
    if not 4<=maximum<=60 or not 1<=seconds<=7200:
        raise ValueError('The sixty-session protocol allows at most60 attempts and7200seconds')
    prereg_path,state_path,lock_path=_paths();lock_path.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
    with lock_path.open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        if not prereg_path.exists():
            if phase!='pilot' or state_path.exists():raise ValueError('Start a new comparison with its pilot')
            ledger=Path(os.environ['COA_PROMPTFOO_LEDGER'])
            if ledger.exists() and legacy.load(ledger).get('attempts'):raise ValueError('A new model protocol requires an unused global ledger')
            codex,host=_codex_binaries()
            prereg={'codex_binary':str(codex),'runtime_binaries':{'codex':legacy.file_hash(codex),'code_mode_host':legacy.file_hash(host),'mcp':legacy.file_hash(Path(os.environ['COA_RAG_BINARY']))},'schema_version':1,'protocol':'promptfoo-model-trials-v1','dataset':DATASET,'schedule':legacy.schedule(case_ids()),'max_attempts':maximum,'max_seconds':seconds,'pins':_pin_files()}
            legacy.write_new(prereg_path,prereg)
            legacy.write_new(state_path,{'preregistration_sha256':legacy.file_hash(prereg_path),'started_at':None,'attempts':[],'stopped':False})
        prereg,state=_load_checked(prereg_path,state_path)
        if prereg['max_attempts']!=maximum or prereg['max_seconds']!=seconds:raise ValueError('Existing model limits cannot change')
        if phase=='remaining':
            approval=Path(os.environ.get('COA_PROMPTFOO_APPROVAL',str(state_path.parent/'PILOT_APPROVED.json')))
            require_approval(legacy.load(approval),state['preregistration_sha256'],state['attempts'])
        elif len(state['attempts'])>=4:raise ValueError('Pilot ends after four attempts; independent hash-bound approval is required')
        if state['stopped'] or any(row['status'] not in ('completed',*legacy.MODEL_FAILURES) for row in state['attempts']):raise ValueError('Uncertain, reserved or failed model attempt exists; stopped without retry')
        index=len(state['attempts'])
        if index>=maximum:raise ValueError('Model attempt budget exhausted')
        expected=prereg['schedule'][index]
        if any(variables.get(key)!=expected[key] for key in ('case_id','model','slot','trial')):raise ValueError('Model slot/order differs from the preregistered interleaving')
        if state['started_at'] is not None and time.time()-state['started_at']>=seconds:raise ValueError('Model protocol time budget exhausted')
        attempt=legacy.reserve(state,prereg['schedule'],time.time());legacy.save_state(state_path,state)
        return {'smoke':False,'state_path':str(state_path),'prereg_path':str(prereg_path),'lock_path':str(lock_path),'slot':attempt['slot'],'case_id':expected['case_id'],'model':expected['model'],'codex_binary':prereg['codex_binary']}


def _bound_native(token, record, native_path, status):
    if status=='failed':return
    live=record['live'];path=Path(native_path).resolve(strict=True)
    if live.get('trace_integrity_status')=='invalid':raise ValueError('Invalid native trace is never a model outcome')
    if record.get('dataset_id')!=DATASET or record.get('case_id')!=token['case_id'] or record.get('requested_model')!=token['model']:
        raise ValueError('Native model execution identity differs from its reserved slot')
    question=next(row['question'] for row in json.loads((DATA/'questions.json').read_text())['cases'] if row['id']==token['case_id'])
    if live.get('question')!=question or (live.get('run') and live['run'].get('model')!=token['model']):raise ValueError('Native model input/model changed')
    expected=json.loads((DATA/'FROZEN.json').read_text())['integrity']
    if record.get('dataset',{}).get('corpus_sha256')!=expected['corpus_sha256']:raise ValueError('Native model corpus changed')
    trace=live.get('audit_trace')
    if not trace or not trace.get('path'):
        diagnostic=live.get('safe_diagnostic') or {}
        if status=='model_contract_violation' and diagnostic.get('validation_reason') in ('missing_required_search','outside_contract_tool') and live.get('trace_integrity_status')!='invalid':
            return path,None
        raise ValueError('Native source audit is missing')
    audit=Path(trace['path']).resolve(strict=True)
    if not audit.is_relative_to(path.parent) or legacy.file_hash(audit)!=trace['sha256']:raise ValueError('Native audit bytes changed or escaped its execution directory')
    lines=[json.loads(line) for line in audit.read_text().splitlines()]
    if status=='model_output_invalid' and not lines:raise ValueError('Invalid output lacks its required search evidence')
    catalogue=[(row['id'],row['date']) for row in expected['sources']]
    if any([(row['id'],row['date']) for row in entry['full_response']['sources']]!=catalogue for entry in lines):raise ValueError('Failed model source catalogue changed')
    return path,audit


def _bind_common(state, record, prereg, audit):
    runtime=record['live'].get('runtime_configuration')
    if not isinstance(runtime,dict) or runtime.get('model')!=record['requested_model'] or runtime.get('binaries')!=prereg['runtime_binaries']:
        raise ValueError('Shared Codex/host/MCP runtime binding changed')
    common={key:value for key,value in runtime.items() if key!='model'}
    if state.get('common_runtime_configuration',common)!=common:raise ValueError('Runtime treatment differs across models')
    identities=[]
    run=record['live'].get('run')
    if run:identities.append(run['manifest']['corpus_identity'])
    if audit:
        identities.extend(json.loads(line)['full_response']['manifest']['corpus_identity'] for line in audit.read_text().splitlines())
    for identity in identities:
        if state.get('retrieval_corpus_identity',identity)!=identity:raise ValueError('Shared corpus/embedding/vector identity changed')
        state['retrieval_corpus_identity']=identity
    state['common_runtime_configuration']=common


def execution_schedule():
    rows=ordered_schedule();phase=os.environ.get('COA_PROMPTFOO_PHASE')
    if phase not in ('pilot','remaining'):return rows
    prereg_path,state_path,_=_paths()
    if not prereg_path.exists():
        if phase=='remaining' or state_path.exists():raise ValueError('Remaining requires the completed original pilot')
        return rows[:4]
    _,state=_load_checked(prereg_path,state_path)
    if state['stopped'] or any(row['status'] not in ('completed',*legacy.MODEL_FAILURES) for row in state['attempts']):
        raise ValueError('Uncertain or reserved model attempt exists; never resume automatically')
    if phase=='remaining':
        approval=Path(os.environ.get('COA_PROMPTFOO_APPROVAL',str(state_path.parent/'PILOT_APPROVED.json')))
        require_approval(legacy.load(approval),state['preregistration_sha256'],state['attempts'])
    start=len(state['attempts'])
    return rows[start:4] if phase=='pilot' else rows[start:]


def finish_model(token, native_record, returncode, native_path):
    status=classify_result(native_record,returncode)
    try:bound=_bound_native(token,native_record,native_path,status)
    except (ValueError,KeyError,OSError,TypeError,json.JSONDecodeError):status='failed';bound=None
    result={'status':status}
    if status in legacy.MODEL_FAILURES:result['model_failure_class']=status
    if token.get('smoke'):return result
    with Path(token['lock_path']).open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        _,state=_load_checked(Path(token['prereg_path']),Path(token['state_path']))
        attempt=state['attempts'][-1]
        if attempt['slot']!=token['slot'] or attempt['status']!='reserved':raise ValueError('Reserved model attempt changed')
        if bound:
            path,audit=bound
            try:_bind_common(state,native_record,legacy.load(Path(token['prereg_path'])),audit)
            except (ValueError,KeyError,TypeError,json.JSONDecodeError):status='failed';result={'status':'failed'}
            attempt.update(record_path=str(path),record_sha256=legacy.file_hash(path))
            if audit:attempt.update(audit_path=str(audit),audit_sha256=legacy.file_hash(audit))
        attempt.update(status=status,finished_at=time.time())
        if status=='failed':state['stopped']=True
        legacy.save_state(Path(token['state_path']),state)
    return result


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('command',choices=['schedule']);parser.parse_args()
    print(json.dumps(execution_schedule(),separators=(',',':')))
