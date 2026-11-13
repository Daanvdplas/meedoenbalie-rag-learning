"""Bind existing Codex rows to preserved native source audits; no model calls."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path

HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('coa_promptfoo_safe_projection',HERE/'provider.py')
provider=importlib.util.module_from_spec(spec);spec.loader.exec_module(provider)


def enrich(input_path,output_path):
    input_path=input_path.resolve(strict=True);output_path=output_path.resolve()
    if output_path.exists() or input_path==output_path:raise ValueError('Choose a new enriched artifact')
    original=input_path.read_bytes();report=json.loads(original);base=input_path.parent
    candidates={}
    paths=sorted(base.rglob('*.jsonl'),key=lambda path:(path.name!='native.jsonl',len(path.parts),str(path)))
    for path in paths:
        if path.name.endswith('.mcp-trace.jsonl'):continue
        try:
            raw=json.loads(path.read_text().strip());live=raw.get('live',raw)
            run_id=live.get('run_id')
            if run_id and live.get('audit_trace',{}).get('path'):candidates.setdefault(run_id,(path,raw))
        except (ValueError,AttributeError):continue
    count=0
    for row in report['results']['results']:
        response=row.get('response',{});output=response.get('output')
        envelope=json.loads(output) if isinstance(output,str) else output
        if not isinstance(envelope,dict) or envelope.get('operation')!='codex':continue
        old=envelope['record'];run_id=old['run_id']
        if run_id not in candidates:raise ValueError('No matching preserved native source audit for '+run_id)
        native_path,raw=candidates[run_id]
        safe=provider._safe_live(raw,artifact_root=base)
        if any(safe.get(key)!=value for key,value in old.items()):raise ValueError('Native evidence differs from original provider output')
        envelope['record']=safe
        response['output']=json.dumps(envelope,ensure_ascii=False) if isinstance(output,str) else envelope
        response.setdefault('metadata',{}).update(native_record_sha256=hashlib.sha256(native_path.read_bytes()).hexdigest(),source_audit_enriched=True)
        count+=1
    report.setdefault('metadata',{})['source_audit_enrichment']={'schema_version':1,'original_eval_id':report['evalId'],'original_raw_sha256':hashlib.sha256(original).hexdigest(),'enriched_rows':count,'model_calls':0}
    output_path.parent.mkdir(parents=True,exist_ok=True)
    with output_path.open('x') as handle:json.dump(report,handle,ensure_ascii=False,separators=(',',':'))
    output_path.chmod(0o600)
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--input',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();result=enrich(args.input,args.output)
    print(json.dumps(result['metadata']['source_audit_enrichment']))


if __name__=='__main__':main()
