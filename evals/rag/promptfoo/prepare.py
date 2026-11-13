"""Prepare only the exact promptfoo-selected local requests, without assessment.

An execution-scoped byte-bound manifest permits shared local model preparation.
This is not promptfoo's output cache and never imports historical measurements.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import time

HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('coa_promptfoo_execution',HERE/'provider.py')
provider=importlib.util.module_from_spec(spec);spec.loader.exec_module(provider)


def prepare(request, output):
    if request.get('schema_version')!=1 or not isinstance(request.get('execution_id'),str) or not request['execution_id']:
        raise ValueError('Expected a versioned, fresh promptfoo selection')
    if output.exists():raise ValueError('Choose a new prepared manifest')
    base=Path(os.environ['COA_PROMPTFOO_ARTIFACT_DIR']).resolve()
    base.mkdir(parents=True,exist_ok=True,mode=0o700)
    binary=Path(os.environ['COA_RAG_BINARY']).resolve(strict=True)
    cache=Path(os.environ.get('COA_RAG_MODEL_CACHE',provider.ROOT/'.model-cache')).resolve()
    groups={}
    for test in request['tests']:
        variables=test['vars'];settings={**provider.DEFAULTS,**{key:variables[key] for key in provider.DEFAULTS if key in variables}}
        if settings['mode']!='local':continue
        if variables['question']!=provider._question(variables['dataset'],variables['case_id']):
            raise ValueError('Selected question differs from frozen input')
        filters=variables.get('filters',{})
        group_key=json.dumps({'dataset':variables['dataset'],'settings':settings,'filters':None if variables['dataset']=='independent-20261103' else filters},sort_keys=True,separators=(',',':'))
        group=groups.setdefault(group_key,{'dataset':variables['dataset'],'settings':settings,'filters':filters,'tests':[]})
        group['tests'].append(variables)
    manifest={'schema_version':1,'execution_id':request['execution_id'],'created_at':time.time(),'execution_mode':'fresh_batch_retrieval','records':{},'groups':len(groups),'runtime_binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}
    for group in groups.values():
        directory=Path(tempfile.mkdtemp(prefix='prepared-',dir=base));directory.chmod(0o700)
        native=directory/'native.jsonl'
        case_ids=sorted({row['case_id'] for row in group['tests']})
        command=[str(binary),'lab','run','--dataset',group['dataset'],'--cases',','.join(case_ids),*provider.common_arguments(group['settings']),'--output',str(native),'--model-cache',str(cache),'--sqlite-dir',str(base/'retrieval-cache')]
        if group['dataset']!='independent-20261103' and group['settings']['apply_filters'] and group['filters']:
            command+=['--query-filters',json.dumps(group['filters'],separators=(',',':'))]
        result=provider._execute(command,provider._child_env(),3600,provider.ROOT)
        if result.returncode!=0:raise RuntimeError('Fresh local preparation failed; private process output not exported')
        rows=[json.loads(line) for line in native.read_text().splitlines()]
        if len(rows)!=len(case_ids) or {row['case_id'] for row in rows}!=set(case_ids):
            raise ValueError('Native preparation returned missing, duplicate or unrequested cases')
        by_case={row['case_id']:row for row in rows}
        for variables in group['tests']:
            row=by_case[variables['case_id']]
            if row['question']!=variables['question']:raise ValueError('Prepared case query differs')
            key=provider.request_key(variables)
            path=directory/(key+'.json')
            if not path.exists():
                with path.open('x') as handle:json.dump(row,handle,ensure_ascii=False,separators=(',',':'))
                path.chmod(0o600)
            manifest['records'][key]={'path':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'run_id':row['run_id'],'case_id':row['case_id']}
    output.parent.mkdir(parents=True,exist_ok=True)
    with output.open('x') as handle:json.dump(manifest,handle,separators=(',',':'))
    output.chmod(0o600)
    return manifest


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--input',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();result=prepare(json.loads(args.input.read_text()),args.output)
    print(json.dumps({'records':len(result['records']),'groups':result['groups'],'execution_id':result['execution_id']}))


if __name__=='__main__':main()
