"""Execution-only promptfoo provider for local Rust retrieval and bounded Codex.

It does not load expectations, references, judges or model API keys. Promptfoo
owns cases/configurations/repeats; assertions.py owns the assessment.
"""
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
METHODS = {'dense','bm25','hybrid_rrf','hybrid_weighted'}
PROFILES = {'documents_sections','documents_activities','sqlite_sections','sqlite_activities'}
MODELS = {'gpt-6.1-sol','gpt-6-luna'}
DEFAULTS = {'mode':'local','method':'bm25','profile':'documents_sections','top_k':3,'candidate_k':30,'context_token_limit':1200,'selection':'sections','apply_filters':False,'mmr':False,'rrf_k':60.0,'vector_weight':.7,'mmr_lambda':.7,'model':'gpt-6.1-sol'}


def _question(dataset, case_id):
    if dataset=='garden':
        paths=[ROOT/'evals/rag/datasets/garden/questions.json']
    elif dataset=='legacy':
        paths=[ROOT/'evals/shared/datasets/legacy'/name for name in ('questions.json','heldout-questions.json')]
    elif dataset in ('independent-20261103','expanded-10-20261105','strengthened-10-20261106'):
        paths=[ROOT/'evals/shared/datasets'/dataset/'questions.json']
    else:
        raise ValueError('Unknown input dataset')
    for path in paths:
        payload=json.loads(path.read_text())
        for row in payload if isinstance(payload,list) else payload['cases']:
            if row['id']==case_id:return row.get('question',row.get('text'))
    raise ValueError('Unknown frozen input case')


def _execute(command, env, timeout, cwd):
    """Reuse only the existing, tested process-tree cancellation utility."""
    path=ROOT/'evals/rag/scripts/model_trials.py'
    spec=importlib.util.spec_from_file_location('coa_process_cleanup',path)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    return module.run_cancellable(command,env,timeout,cwd=cwd)


def _child_env(model=None):
    env=dict(os.environ)
    for key in ('OPENAI_API_KEY','COA_OPENAI_API_KEY_FILE','OPENAI_BASE_URL','OPENAI_API_BASE','ORQ_API_KEY','ORQ_API_KEY_FILE'):
        env.pop(key,None)
    if model is not None:env['COA_CODEX_MODEL']=model
    return env


def _reserve(dataset, case_id, model):
    if os.environ.get('COA_PROMPTFOO_ALLOW_CODEX')!='1':
        raise ValueError('Codex requires explicit COA_PROMPTFOO_ALLOW_CODEX=1')
    path=Path(os.environ['COA_PROMPTFOO_LEDGER']).resolve()
    maximum=int(os.environ.get('COA_PROMPTFOO_MAX_GENERATIONS','8'))
    seconds=int(os.environ.get('COA_PROMPTFOO_MAX_SECONDS','900'))
    if not 1<=maximum<=200 or not 1<=seconds<=14400:
        raise ValueError('Codex limits must be 1..200 sessions and 1..14400 seconds')
    descriptor=os.open(path,os.O_RDWR|os.O_CREAT|getattr(os,'O_NOFOLLOW',0),0o600)
    with os.fdopen(descriptor,'r+') as handle:
        fcntl.flock(handle,fcntl.LOCK_EX)
        raw=handle.read();now=time.time()
        state=json.loads(raw) if raw else {'schema_version':1,'started_at':now,'max_generations':maximum,'max_seconds':seconds,'attempts':[],'stopped':False}
        if state.get('schema_version')!=1 or state.get('max_generations')!=maximum or state.get('max_seconds')!=seconds:
            raise ValueError('Existing Codex ledger limits cannot be changed or reset')
        if state.get('stopped') or any(row['status']=='reserved' for row in state['attempts']):
            raise ValueError('Codex batch stopped after a failed or uncertain attempt')
        if now>=state['started_at']+seconds or len(state['attempts'])>=maximum:
            raise ValueError('Codex session/time budget exhausted')
        attempt={'id':uuid.uuid4().hex,'dataset':dataset,'case_id':case_id,'model':model,'status':'reserved','started_at':now}
        state['attempts'].append(attempt)
        handle.seek(0);json.dump(state,handle,separators=(',',':'));handle.truncate();handle.flush();os.fsync(handle.fileno())
    return path,attempt['id'],maximum,seconds,state['started_at']+seconds


def _model_controls():
    spec=importlib.util.spec_from_file_location('coa_model_controls',HERE/'model_control.py')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    return module


def _finish(path, attempt_id, completed):
    with path.open('r+') as handle:
        fcntl.flock(handle,fcntl.LOCK_EX);state=json.load(handle)
        attempt=next(row for row in state['attempts'] if row['id']==attempt_id)
        status='completed' if completed is True else completed if completed in ('model_output_invalid','model_contract_violation') else 'failed'
        attempt.update(status=status,finished_at=time.time())
        if status=='failed':state['stopped']=True
        handle.seek(0);json.dump(state,handle,separators=(',',':'));handle.truncate();handle.flush();os.fsync(handle.fileno())


def _select(record, names):
    return {key:record[key] for key in names if key in record}


def _safe_live(record, artifact_root=None):
    """Expose observations only, keeping local runtime/trace paths private."""
    live=record.get('live',record)
    clean=_select(live,('schema_version','execution_kind','run_id','question','config_id','config','filters','status','budget_id','generation_session_index','ai_judge_calls','answer_quality_status','source_guard_passed','elapsed_ms','payload_mode','transport_treatment_id','trace_integrity_status'))
    if live.get('audit_trace'):
        clean['audit_trace']=_select(live['audit_trace'],('session_id','sha256','trace_bytes','payload_bytes','successful_searches'))
    if live.get('audit_trace'):
        trace=live['audit_trace']
        path=Path(trace['path']).resolve(strict=True)
        base=Path(artifact_root or os.environ['COA_PROMPTFOO_ARTIFACT_DIR']).resolve(strict=True)
        if not path.is_relative_to(base) or not path.is_file() or path.stat().st_size>16*1024*1024:
            raise ValueError('Native audit is outside the private execution')
        raw=path.read_bytes()
        if len(raw)!=trace['trace_bytes'] or hashlib.sha256(raw).hexdigest()!=trace['sha256'] or not raw.endswith(b'\n'):
            raise ValueError('Native source audit bytes changed')
        allowed={'schema_version','session_id','call_index','request_id','trace_id','payload_mode','payload_bytes','full_response_sha256','full_response'}
        if any(set(json.loads(line))!=allowed for line in raw.decode().splitlines()):
            raise ValueError('Unknown native source audit fields')
        clean['source_audit_jsonl']=raw.decode()
    if live.get('error') is not None:clean['error']=live['error']
    if live.get('safe_diagnostic') is not None:
        clean['safe_diagnostic']=_select(live['safe_diagnostic'],('category','stage','exit_code','json_event_lines_valid','event_types','item_types','provider_error_code','validation_reason','tool_event_category'))
    if live.get('run'):
        run=live['run']
        clean['run']=_select(run,('id','status','question','answer','tool_calls','sources','as_of','client_version','model','elapsed_ms','usage'))
    else:
        clean['run']=None
    return clean


def request_key(variables):
    selected={key:variables.get(key,value) for key,value in DEFAULTS.items()}
    selected.update({key:variables[key] for key in ('dataset','case_id','question')})
    selected['filters']=variables.get('filters',{})
    return hashlib.sha256(json.dumps(selected,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()


def common_arguments(settings):
    return ['--methods',settings['method'],'--profiles',settings['profile'],'--top-k',str(settings['top_k']),'--candidate-k',str(settings['candidate_k']),'--context-limit',str(settings['context_token_limit']),'--selection',settings['selection'],'--apply-filters','on' if settings['apply_filters'] else 'off','--mmr','on' if settings['mmr'] else 'off','--rrf-k',str(settings['rrf_k']),'--vector-weight',str(settings['vector_weight']),'--mmr-lambda',str(settings['mmr_lambda'])]


def _prepared_record(variables):
    manifest=json.loads(Path(os.environ['COA_PROMPTFOO_PREPARED']).read_text())
    if manifest.get('schema_version')!=1 or not os.environ.get('COA_PROMPTFOO_EXECUTION_ID') or manifest.get('execution_id')!=os.environ['COA_PROMPTFOO_EXECUTION_ID']:
        raise ValueError('Prepared retrieval belongs to a different execution')
    row=manifest['records'][request_key(variables)]
    path=Path(row['path']).resolve(strict=True)
    base=Path(os.environ['COA_PROMPTFOO_ARTIFACT_DIR']).resolve(strict=True)
    if not path.is_relative_to(base) or not path.is_file() or path.stat().st_size>2*1024*1024:
        raise ValueError('Prepared retrieval path is outside this private execution')
    raw=path.read_bytes()
    if hashlib.sha256(raw).hexdigest()!=row['sha256']:
        raise ValueError('Prepared retrieval record changed')
    return json.loads(raw)


def response_for(raw,variables,settings,operation,execution_mode):
    record=_safe_live(raw) if operation=='codex' else raw
    status=record.get('status',record.get('execution_status','unknown'))
    if status!='completed':
        return {'error':'Native execution status: '+str(status),'metadata':{'case_id':variables['case_id'],'dataset':variables['dataset'],'mode':settings['mode'],'status':status}}
    envelope={'schema_version':1,'operation':operation,'dataset_id':variables['dataset'],'case_id':variables['case_id'],'question':variables['question'],'status':status,'record':record}
    metadata={'run_id':record.get('run_id',operation+'-'+uuid.uuid4().hex),'case_id':variables['case_id'],'dataset':variables['dataset'],'method':settings['method'],'profile':settings['profile'],'mode':settings['mode'],'model':settings['model'] if operation=='codex' else None,'answer_quality':'not_assessed','execution_mode':execution_mode}
    usage={'numRequests':1 if operation=='codex' else 0}
    native_usage=(record.get('run') or {}).get('usage') if operation=='codex' else None
    if native_usage:
        usage.update(prompt=native_usage['input_tokens'],completion=native_usage['output_tokens'],total=native_usage['input_tokens']+native_usage['output_tokens'])
        if native_usage.get('cached_input_tokens') is not None:usage['cached']=native_usage['cached_input_tokens']
    return {'output':envelope,'metadata':metadata,'tokenUsage':usage,'cached':False}


def call_api(prompt, options, context):
    """Ignore rendered prompt text; forward only validated frozen question input."""
    reservation=None;model_token=None;controls=None;output=None
    try:
        variables=context.get('vars',{})
        settings={**DEFAULTS,**options.get('config',{}),**{key:variables[key] for key in DEFAULTS if key in variables}}
        dataset=variables['dataset'];case_id=variables['case_id'];question=variables['question']
        if question!=_question(dataset,case_id):raise ValueError('Question differs from frozen input')
        mode=settings['mode'];method=settings['method'];profile=settings['profile']
        if mode not in ('local','codex','garden','compact','compact-live'):raise ValueError('Unknown execution mode')
        if profile not in PROFILES or method not in (METHODS|({'keyword_overlap'} if mode=='garden' else set())):
            raise ValueError('Unsupported method/profile')
        if settings['model'] not in MODELS:raise ValueError('Unknown Codex model')
        if not isinstance(settings['top_k'],int) or not 1<=settings['top_k']<=6:raise ValueError('Invalid top-k')
        if context.get('test',{}).get('metadata',{}).get('suite')=='models':
            if mode!='codex':raise ValueError('Models require the canonical Codex execution mode')
            if os.environ.get('COA_PROMPTFOO_ALLOW_CODEX')!='1':raise ValueError('Models require explicit Codex opt-in before protocol reservation')
            controls=_model_controls();model_token=controls.authorize_model({**variables,**settings})
        if mode in ('codex','compact-live'):
            reservation=_reserve(dataset,case_id,settings['model'])
        if mode=='local' and os.environ.get('COA_PROMPTFOO_PREPARED'):
            return response_for(_prepared_record(variables),variables,settings,'retrieve','fresh_batch_retrieval')
        binary=Path(os.environ.get('COA_RAG_BINARY',ROOT/'target/debug/meedoen-retrieval')).resolve(strict=True)
        if not binary.is_file() or not os.access(binary,os.X_OK):raise ValueError('COA_RAG_BINARY must be executable')
        base=Path(os.environ.get('COA_PROMPTFOO_ARTIFACT_DIR',tempfile.gettempdir()+'/coa-promptfoo')).resolve()
        base.mkdir(parents=True,exist_ok=True,mode=0o700)
        directory=Path(tempfile.mkdtemp(prefix='case-',dir=base));directory.chmod(0o700)
        output=directory/'native.jsonl'
        cache=Path(os.environ.get('COA_RAG_MODEL_CACHE',ROOT/'.model-cache')).resolve()
        command=[str(binary),'lab'];cwd=ROOT
        common=common_arguments(settings)
        if mode=='compact':
            relative=variables['replay_path'];source=(ROOT/relative).resolve(strict=True)
            pins=json.loads((HERE/'data-pins.json').read_text())['files']
            if not source.is_relative_to(ROOT/'evals/rag/fixtures') or pins.get(relative)!=hashlib.sha256(source.read_bytes()).hexdigest() or variables['input_sha256']!=pins[relative]:raise ValueError('Replay input changed or is outside canonical fixtures')
            command+=['compact-project','--input',str(source)]
        elif mode=='garden':
            command+=['garden-search','--question',question,'--method',method,'--top-k',str(settings['top_k'])];cwd=cache.parent
        elif mode=='local':
            command+=['run','--dataset',dataset,'--cases',case_id,*common,'--output',str(output),'--model-cache',str(cache),'--sqlite-dir',str(base/'retrieval-cache')]
            if dataset!='independent-20261103' and settings['apply_filters'] and variables.get('filters'):
                command+=['--query-filters',json.dumps(variables['filters'],separators=(',',':'))]
        else:
            ledger_path,attempt_id,maximum,seconds,deadline=reservation
            native_key=hashlib.sha256((dataset+settings['model']+mode+str(settings.get('payload_mode','compact'))).encode()).hexdigest()[:16]
            native_ledger=ledger_path.with_name(ledger_path.stem+'-native-'+native_key+'.json')
            if mode=='compact-live':
                command+=['ask','--live','--question',question,*common,'--payload-mode',variables.get('payload_mode','compact')]
            else:
                command+=['agent-run','--live','--cases',case_id,'--methods',method]
                if dataset in ('expanded-10-20261105','strengthened-10-20261106'):command+=['--dataset',dataset]
            command+=['--output',str(output),'--ledger',str(native_ledger),'--model-cache',str(cache),'--max-generations',str(maximum),'--max-seconds',str(seconds)]
        timeout=150
        if reservation:
            timeout=min(timeout,reservation[4]-time.time()-5)
            if timeout<=0:raise ValueError('No shared time remains for a safely bounded Codex call')
        child_env=_child_env(settings['model'] if reservation else None)
        if model_token and model_token.get('codex_binary'):
            child_env['COA_CODEX_BIN']=model_token['codex_binary']
        result=_execute(command,child_env,timeout,cwd)
        raw=json.loads(result.stdout) if mode in ('garden','compact') else json.loads(output.read_text().strip())
        operation='compact' if mode=='compact' else 'garden' if mode=='garden' else 'codex' if reservation else 'retrieve'
        if model_token:
            # Validate private audit bytes before recording a successful attempt.
            _safe_live(raw)
            verdict=controls.finish_model(model_token,raw,result.returncode,output);model_token=None
            if verdict['status'] in ('model_output_invalid','model_contract_violation'):
                failure=verdict['status']
                record=_safe_live(raw)
                envelope={'schema_version':1,'operation':'codex','dataset_id':dataset,'case_id':case_id,'question':question,'status':'failed','record':record,'model_failure_evidence':{'returncode':result.returncode,'requested_model':raw['requested_model'],'native_record_sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'model_failure_class':failure}}
                _finish(reservation[0],reservation[1],failure);reservation=None
                return {'output':envelope,'metadata':{'run_id':record.get('run_id'),'case_id':case_id,'dataset':dataset,'method':method,'profile':profile,'mode':mode,'model':settings['model'],'model_failure_class':failure,'execution_mode':'fresh_runtime_execution'},'tokenUsage':{'numRequests':1},'cached':False}
            if verdict['status']!='completed':raise ValueError('Model protocol could not verify the native execution')
        if result.returncode!=0:
            raise RuntimeError('Native runtime failed; private process output was not exported')
        response=response_for(raw,variables,settings,operation,'offline_projection_replay' if mode=='compact' else 'fresh_runtime_execution')
        if reservation:
            _finish(reservation[0],reservation[1],'error' not in response);reservation=None
        return response
    except (ValueError,KeyError,OSError,RuntimeError,subprocess.SubprocessError,json.JSONDecodeError):
        if model_token:
            try:controls.finish_model(model_token,None,-1,output)
            except (ValueError,KeyError,OSError,TypeError):pass
        if reservation:
            _finish(reservation[0],reservation[1],False)
        return {'error':'COA runtime/configuration failed; no retry or private process output was exported'}
