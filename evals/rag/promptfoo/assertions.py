"""Canonical promptfoo source assertions; protected labels never enter providers.

A promptfoo success on a no-target row means execution integrity only. Such rows
have no source_complete/goal_coverage named metric and stay outside quality
ratios. This module never calls a model or the historical Rust assessor.
"""
from functools import lru_cache
import hashlib
import json
import math
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
DATASETS = {'legacy', 'independent-20261103', 'expanded-10-20261105', 'strengthened-10-20261106', 'garden'}


def _read(relative):
    path = ROOT / relative
    pins = json.loads((HERE / 'data-pins.json').read_text())['files']
    raw = path.read_bytes()
    if pins.get(relative) != hashlib.sha256(raw).hexdigest():
        raise ValueError('Frozen evaluation input changed: ' + relative)
    return json.loads(raw)


def _dataset_files(dataset):
    if dataset not in DATASETS:
        raise ValueError('Unknown dataset')
    if dataset == 'garden':
        return [('evals/rag/datasets/garden/questions.json', 'evals/rag/datasets/garden/expectations.json')]
    base = 'evals/shared/datasets/' + dataset
    if dataset == 'legacy':
        return [(base + '/questions.json', base + '/expectations.json'), (base + '/heldout-questions.json', base + '/heldout-expectations.json')]
    return [(base + '/questions.json', base + '/expectations.json')]


def load_case(dataset, case_id):
    """Return existing assessment-only labels, never a new generated reference."""
    for questions_path, labels_path in _dataset_files(dataset):
        questions, labels = _read(questions_path), _read(labels_path)
        cases = questions if isinstance(questions, list) else questions['cases']
        question = next((row for row in cases if row['id'] == case_id), None)
        if question is None:
            continue
        key = 'question_id' if dataset == 'garden' else 'case_id'
        criteria = next((row for row in labels['cases'] if row[key] == case_id), None)
        if criteria is None:
            raise ValueError('Missing protected reference case')
        result = dict(criteria)
        result['question'] = question.get('question', question.get('text'))
        result['question_sha256'] = hashlib.sha256((ROOT / questions_path).read_bytes()).hexdigest()
        result['reference_sha256'] = hashlib.sha256((ROOT / labels_path).read_bytes()).hexdigest()
        result['corpus_sha256'] = _read('evals/shared/datasets/legacy/questions.json')['corpus_sha256'] if dataset == 'garden' else questions['corpus_sha256']
        result['as_of'] = labels.get('as_of') if dataset == 'garden' else questions['as_of']
        if dataset == 'garden':
            result['facts'] = [{'id':'source-' + str(i), 'evidence_alternatives': [[dict(zip(('source_id','section_id'), key.split(':')))]]} for i,key in enumerate(criteria['required_source_ids'])]
            result['relevance_labels_complete'] = False
            result['relevant_sections'] = []
            result['required_unknowns'] = criteria.get('unknown_flags', [])
        return result
    raise ValueError('Unknown case in frozen dataset')


def _key(reference):
    return reference['source_id'], reference.get('section_id', reference.get('id'))


def goal_metrics(goals, offered):
    alternatives = [[{_key(ref) for ref in option} for option in goal['evidence_alternatives']] for goal in goals if goal.get('evidence_alternatives')]
    if not alternatives:
        return {'target_goals': 0, 'covered_goals': 0, 'source_complete': None, 'goal_coverage': None, 'minimum_required_sections': None}
    covered = sum(any(option.issubset(offered) for option in options) for options in alternatives)
    frontier = [set()]
    for options in alternatives:
        next_frontier = []
        for previous in frontier:
            for option in options:
                union = previous | option
                if any(other.issubset(union) for other in next_frontier):
                    continue
                next_frontier = [other for other in next_frontier if not union.issubset(other)]
                next_frontier.append(union)
        frontier = next_frontier
    return {'target_goals':len(alternatives), 'covered_goals':covered, 'source_complete':covered == len(alternatives), 'goal_coverage':covered / len(alternatives), 'minimum_required_sections':min(map(len, frontier))}


@lru_cache(maxsize=2)
def canonical_sources(expanded=False):
    names = ['document.md','bicycle-workshop.md','language-cafe.md','community-cooking.md','walking.md']
    if expanded:
        names += [f'expanded-10-20261105/activity-{i:03}.md' for i in range(6, 11)]
    pins = json.loads((HERE / 'data-pins.json').read_text())['files']
    sources = {}
    for name in names:
        relative = 'fixtures/' + name
        raw = (ROOT / relative).read_bytes()
        if pins.get(relative) != hashlib.sha256(raw).hexdigest():
            raise ValueError('Canonical source changed: ' + relative)
        text = raw.decode()
        pieces = re.split(r'^## (S\d+) — (.+)\n', text, flags=re.M)
        header = {'source_id':re.search(r'^ID: (.+)$',text,re.M)[1], 'source_title':re.search(r'^# (.+)$',text,re.M)[1], 'source_date':re.search(r'^Brondatum: (.+)$',text,re.M)[1]}
        for i in range(1,len(pieces),3):
            section = dict(header, id=pieces[i], heading=pieces[i+1], text=pieces[i+2].strip())
            sources[_key(section)] = section
    return sources


def _ordered_section(section):
    return {key:section[key] for key in ('source_id','source_date','source_title','id','heading','text')}


def _json_bytes(value):
    return json.dumps(value,ensure_ascii=False,separators=(',',':')).encode()


def _validate_config(actual, variables):
    if actual.get('profile') != variables.get('profile'):
        raise ValueError('Storage profile differs from requested configuration')
    config=actual['retrieval']
    for key in ('method','top_k','candidate_k','context_token_limit','selection','apply_filters','mmr','rrf_k','vector_weight','mmr_lambda'):
        if key not in variables or config.get(key) != variables[key]:
            raise ValueError('Retrieval configuration differs from request: '+key)
    return config


def _validate_local(record, variables, criteria, canonical):
    config=_validate_config(record['config'],variables)
    identity=record.get('dataset',{})
    if identity.get('corpus_sha256')!=criteria['corpus_sha256'] or identity.get('as_of')!=criteria['as_of']:
        raise ValueError('Recorded dataset corpus/date differs from frozen input')
    outcome=record['outcome']
    if outcome.get('version')!='retrieval-v1' or outcome.get('config')!=config:
        raise ValueError('Retrieval trace version/configuration differs')
    source_sections=[_ordered_section(section) for section in canonical.values()]
    expected_source=hashlib.sha256(_json_bytes(source_sections)).hexdigest()
    inputs=[section['source_title']+'\n'+section['heading']+'\n'+section['text'] for section in source_sections]
    expected_embedding=hashlib.sha256(_json_bytes(inputs)).hexdigest()
    corpus_identity=outcome.get('corpus_identity',{})
    if corpus_identity.get('source_sha256')!=expected_source or corpus_identity.get('embedding_inputs_sha256')!=expected_embedding:
        raise ValueError('Prepared corpus/source embedding inputs differ')
    preparation=record.get('preparation',{})
    if preparation.get('corpus_identity')!=corpus_identity or preparation.get('actual_profile')!=record['config']['profile']:
        raise ValueError('Actual preparation differs from requested corpus/profile')
    candidates=outcome.get('candidates',[])
    candidate_map={_key(candidate['key']):candidate for candidate in candidates}
    if len(candidate_map)!=len(candidates) or not set(candidate_map).issubset(canonical):
        raise ValueError('Duplicate or unknown candidate sections')
    if outcome.get('work',{}).get('union_candidates')!=len(candidates):
        raise ValueError('Candidate work trace is inconsistent')
    uses_dense=config['method']!='bm25';uses_keyword=config['method']!='dense'
    for rank_name,score_name,used in [('dense_rank','dense_similarity',uses_dense),('keyword_rank','bm25_score',uses_keyword)]:
        ranks=[candidate['scores'][rank_name] for candidate in candidates if candidate['scores'].get(rank_name) is not None]
        if len(ranks)>config['candidate_k'] or set(ranks)!=set(range(1,len(ranks)+1)) or len(ranks)!=len(set(ranks)):
            raise ValueError('Candidate ranks are inconsistent')
        for candidate in candidates:
            scores=candidate['scores'];score=scores.get(score_name)
            if (score is not None)!=used or (not used and scores.get(rank_name) is not None):
                raise ValueError('Candidate scoring method differs')
            if score is not None and (not math.isfinite(score) or (score_name=='dense_similarity' and not -1<=score<=1) or (score_name=='bm25_score' and score<0)):
                raise ValueError('Invalid retrieval score')
            if score_name=='bm25_score' and scores.get(rank_name) is not None and not score>0:
                raise ValueError('Nonpositive keyword candidate rank')
    if any(not math.isfinite(candidate['scores']['rank_score']) for candidate in candidates):
        raise ValueError('Invalid ranking score')
    if candidates!=sorted(candidates,key=lambda candidate:(-candidate['scores']['rank_score'],_key(candidate['key']))):
        raise ValueError('Candidate order differs from recorded rank scores')
    seeds=[_key(ref) for ref in outcome.get('seeds',[])]
    hits=outcome.get('hits',[]);offered=[_key(hit['section']) for hit in hits]
    actual_seeds=[_key(hit['section']) for hit in hits if hit.get('inclusion')=='seed']
    if len(seeds)!=len(set(seeds)) or not set(seeds).issubset(candidate_map) or seeds!=actual_seeds or len(seeds)>config['top_k']:
        raise ValueError('Seed selection is not bound to candidates and offered hits')
    if config['selection']=='sections' and (offered!=seeds or len(hits)>config['top_k']):
        raise ValueError('Sections selection contains unselected or excess source hits')
    seed_activities={source for source,_ in seeds}
    for hit in hits:
        key=_key(hit['section']);candidate=candidate_map.get(key)
        if hit.get('scores')!=(candidate['scores'] if candidate else None):
            raise ValueError('Offered scores differ from candidates')
        if hit.get('inclusion') not in ('seed','activity_expansion') or (hit.get('inclusion')=='activity_expansion' and key[0] not in seed_activities):
            raise ValueError('Invalid activity expansion')
    omitted=outcome.get('omitted',[]);omitted_keys=[_key(row['key']) for row in omitted]
    if len(omitted_keys)!=len(set(omitted_keys)) or not set(omitted_keys).issubset(canonical) or set(omitted_keys)&set(offered) or (config['selection']=='sections' and omitted):
        raise ValueError('Invalid omitted-section trace')
    if any(not str(row.get('reason','')).strip() or _key(row['key'])[0] not in seed_activities for row in omitted):
        raise ValueError('Unbound omitted activity section')
    tokens=sum((len(_json_bytes(_ordered_section(hit['section'])).decode())+3)//4 for hit in hits)
    if outcome.get('token_estimator')!='unicode-scalar-ceil-div4-json-section-v1' or outcome.get('estimated_context_tokens')!=tokens or tokens>config['context_token_limit']:
        raise ValueError('Offered context differs from the measured token budget')
    layout=preparation.get('layout',{})
    if layout.get('canonical_sections')!=len(canonical):
        raise ValueError('Declared storage layout does not contain the canonical corpus')
    activities=record['config']['profile'].endswith('_activities')
    if layout.get('activity_records')!=(len({key[0] for key in canonical}) if activities else 0) or layout.get('activity_section_links')!=(len(canonical) if activities else 0):
        raise ValueError('Storage structure differs from requested layout')
    request_filters=variables.get('filters',{'weekdays':[],'environment':None,'time':None})
    normalized={'weekdays':[],'environment':None,'time':None,**request_filters}
    if record.get('request_filters')!=normalized:
        raise ValueError('Typed input filters differ from requested user constraints')
    filters=outcome.get('filters',{})
    requested=bool(normalized['weekdays'] or normalized['environment'] is not None or normalized['time'] is not None)
    if filters.get('requested')!=requested or filters.get('applied')!=(config['apply_filters'] and requested):
        raise ValueError('Typed filter application trace differs')
    if filters.get('applied') and not {key[0] for key in offered}.issubset(set(filters.get('matched_activity_ids',[]))):
        raise ValueError('Offered source does not match the applied filter trace')


def _raw_json_field(text, field):
    """Extract exact native serialized value bytes without reformatting floats."""
    decoder=json.JSONDecoder();index=0
    while text[index].isspace():index+=1
    if text[index]!='{':raise ValueError('Audit row is not a JSON object')
    index+=1
    while index<len(text):
        while text[index].isspace():index+=1
        if text[index]=='}':break
        name,index=decoder.raw_decode(text,index)
        while text[index].isspace():index+=1
        if text[index]!=':':raise ValueError('Invalid audit field separator')
        index+=1
        while text[index].isspace():index+=1
        start=index;_,index=decoder.raw_decode(text,index)
        if name==field:return text[start:index]
        while index<len(text) and text[index].isspace():index+=1
        if text[index]==',':index+=1
        elif text[index]=='}':break
        else:raise ValueError('Invalid audit object delimiter')
    raise ValueError('Missing full audit response')


def _audited_responses(live,calls):
    raw=live.get('source_audit_jsonl');summary=live.get('audit_trace',{})
    if not isinstance(raw,str) or not raw.endswith('\n') or len(raw.encode())!=summary.get('trace_bytes') or hashlib.sha256(raw.encode()).hexdigest()!=summary.get('sha256'):
        raise ValueError('Missing or changed SHA-bound source audit')
    lines=raw.splitlines();rows=[json.loads(line) for line in lines]
    successes=[call for call in calls if call.get('status')=='completed']
    if len(rows)!=len(successes) or len(rows)!=summary.get('successful_searches') or sum(row['payload_bytes'] for row in rows)!=summary.get('payload_bytes'):
        raise ValueError('Source audit call/byte counts differ')
    restored=[];audit_index=0
    for call in calls:
        if call.get('status')!='completed':
            if call.get('result') is not None:raise ValueError('Failed search cannot supply source evidence')
            restored.append((call,None));continue
        audit=rows[audit_index];line=lines[audit_index];audit_index+=1
        index=audit_index;session=summary['session_id']
        expected_trace={'session_id':session,'call_index':index,'request_id':f'{session}-request-{index}','trace_id':f'{session}-trace-{index}','full_response_sha256':audit['full_response_sha256']}
        if audit.get('schema_version')!=1 or audit.get('payload_mode')!=live['payload_mode'] or any(audit.get(key)!=value for key,value in expected_trace.items()):
            raise ValueError('Source audit session/order/request identity differs')
        full=audit['full_response'];full_text=_raw_json_field(line,'full_response')
        if json.loads(full_text)!=full or hashlib.sha256(full_text.encode()).hexdigest()!=audit['full_response_sha256']:
            raise ValueError('Complete source audit digest differs')
        if full.get('question')!=call.get('query') or full.get('top_k')!=call.get('top_k'):
            raise ValueError('Audited source query differs from actual tool call')
        if live['payload_mode']=='full':
            projected=full;payload_bytes=len(full_text.encode())
        else:
            projected={**{key:full[key] for key in ('question','top_k','sources','as_of','manifest')},'schema_version':2,'payload_mode':'compact','hits':[{'section':hit['section']} for hit in full['hits']],'trace':expected_trace}
            payload_bytes=len(json.dumps(projected,sort_keys=True,ensure_ascii=False,separators=(',',':')).encode())
        if call.get('result')!=projected or audit['payload_bytes']!=payload_bytes:
            raise ValueError('Visible full/compact payload differs from its exact audited projection')
        restored.append((call,full))
    return restored


def _observations(envelope, variables, criteria, canonical):
    record = envelope['record']
    if envelope['operation'] == 'retrieve':
        if record.get('execution_status') != 'completed' or record.get('execution_kind') != 'local_measured':
            raise ValueError('Retrieval was not a completed measured execution')
        if record.get('question') != envelope['question'] or record.get('case_id') != envelope['case_id']:
            raise ValueError('Recorded input identity differs')
        outcome = record.get('outcome')
        if not isinstance(outcome,dict) or outcome.get('query') != envelope['question']:
            raise ValueError('Missing or mismatched retrieval trace')
        _validate_local(record,variables,criteria,canonical)
        hits = [hit['section'] for hit in outcome['hits']]
        seeds = [_key(ref) for ref in outcome.get('seeds', [])]
        offered = [_key(section) for section in hits]
        if len(offered) != len(set(offered)) or len(seeds) != len(set(seeds)) or not set(seeds).issubset(offered):
            raise ValueError('Invalid duplicate or unoffered seed references')
        config = record['config']['retrieval']
        if len(seeds) > config['top_k'] or outcome.get('estimated_context_tokens', 0) > config['context_token_limit']:
            raise ValueError('Retrieval violated its declared context limits')
        return hits, seeds, config, []
    if envelope['operation'] == 'codex':
        live = record.get('live', record)
        if live.get('status') != 'completed' or not isinstance(live.get('run'),dict) or live.get('question')!=envelope['question']:
            raise ValueError('Codex session did not complete with the requested input')
        config=_validate_config(live['config'],variables)
        run=live['run']
        if run.get('model')!=variables.get('model') or run.get('question')!=envelope['question'] or run.get('status')!='completed' or live.get('source_guard_passed') is not True:
            raise ValueError('Actual Codex model/input/source guard differs from request')
        if live.get('schema_version')!=2 or live.get('trace_integrity_status')!='validated':
            raise ValueError('New Codex run lacks a validated audit trace')
        if live.get('payload_mode')!=variables.get('payload_mode','compact'):
            raise ValueError('Codex transport payload differs from requested treatment')
        sources=[]
        for source in canonical.values():
            info={'id':source['source_id'],'title':source['source_title'],'date':source['source_date']}
            if info not in sources:sources.append(info)
        if run.get('sources')!=sources or run.get('as_of')!=criteria['as_of']:
            raise ValueError('Codex source catalogue/date differs from the canonical corpus')
        calls=run.get('tool_calls')
        if not isinstance(calls,list) or not 1<=len(calls)<=4:
            raise ValueError('Missing or over-limit Codex search trace')
        hits=[]
        for call,full_response in _audited_responses(live,calls):
            if call.get('server')!='coa' or call.get('tool')!='search_document':
                raise ValueError('Untrusted tool/server cannot supply retrieval evidence')
            result=full_response
            if call.get('status')!='completed':
                if result is not None:raise ValueError('Failed search cannot offer source evidence')
                continue
            if not isinstance(result,dict) or result.get('question')!=call.get('query') or result.get('top_k')!=config['top_k'] or call.get('top_k')!=config['top_k']:
                raise ValueError('Codex search response differs from its actual request')
            if result.get('sources')!=sources or result.get('as_of')!=criteria['as_of']:
                raise ValueError('Tool source catalogue/date differs')
            outcome=result.get('outcome',{})
            manifest=result.get('manifest',{})
            if result.get('hits')!=outcome.get('hits') or manifest.get('config')!=live['config'] or manifest.get('corpus_identity')!=outcome.get('corpus_identity') or outcome.get('query')!=call['query']:
                raise ValueError('Tool-visible evidence differs from its complete audit')
            native={'config':live['config'],'dataset':{'corpus_sha256':criteria['corpus_sha256'],'as_of':criteria['as_of']},'outcome':outcome,'preparation':result.get('preparation'),'request_filters':live.get('filters')}
            _validate_local(native,variables,criteria,canonical)
            hits += [hit['section'] for hit in result['hits']]
        return hits, [], config, calls
    if envelope['operation'] == 'garden':
        if record.get('status')!='completed' or record.get('question',record.get('query'))!=envelope['question'] or record.get('method')!=variables.get('method') or record.get('top_k')!=variables.get('top_k'):
            raise ValueError('Garden runtime/request identity differs')
        manifest=record.get('source_manifest',{})
        if manifest.get('corpus_sha256')!=criteria['corpus_sha256'] or manifest.get('as_of')!=criteria['as_of']:
            raise ValueError('Garden source corpus/date differs')
        hits=[hit['section'] for hit in record['hits']]
        if len(hits)>variables['top_k'] or len({_key(hit) for hit in hits})!=len(hits):
            raise ValueError('Invalid garden source selection')
        return hits, [_key(hit) for hit in hits], {'top_k':record['top_k'],'selection':'sections','context_token_limit':1000000}, []
    raise ValueError('Unknown source evaluation operation')


def measure(envelope, variables):
    """Compute source measurements from observed evidence and protected labels."""
    if envelope.get('status')!='completed':
        raise ValueError('Provider envelope is not a completed execution')
    if envelope.get('schema_version') != 1:
        raise ValueError('Unknown promptfoo output schema')
    dataset = envelope['dataset_id']; case_id=envelope['case_id']
    if dataset != variables.get('dataset') or case_id != variables.get('case_id'):
        raise ValueError('Provider output belongs to a different test')
    criteria = load_case(dataset,case_id)
    if envelope['question'] != variables.get('question') or envelope['question'] != criteria['question']:
        raise ValueError('Question differs from the frozen input')
    canonical = canonical_sources(dataset in ('expanded-10-20261105','strengthened-10-20261106'))
    hits,seeds,config,calls = _observations(envelope,variables,criteria,canonical)
    for hit in hits:
        if canonical.get(_key(hit)) != hit:
            raise ValueError('Offered source text, date or identity is not canonical')
    offered={_key(hit) for hit in hits}
    result=goal_metrics(criteria.get('facts',[]),offered)
    support={_key(ref) for fact in criteria.get('facts',[]) for option in fact.get('evidence_alternatives',[]) for ref in option}
    result.update(execution_integrity=True, reference_sha256=criteria['reference_sha256'], question_sha256=criteria['question_sha256'], offered_sections=len(offered), source_quality_applicable=result['target_goals']>0, answer_quality='not_assessed', unknown_obligations=len(criteria.get('required_unknowns',[])))
    if result['target_goals'] and envelope['operation']!='codex':
        result['first_goal_support_reciprocal_rank']=next((1/(i+1) for i,key in enumerate(seeds) if key in support),0)
    if envelope['operation']!='codex' and result['minimum_required_sections'] is not None and config.get('selection')=='sections':
        result['section_capacity_limit']=config['top_k']
        result['goal_set_exceeds_capacity']=result['minimum_required_sections']>config['top_k']
    if envelope['operation']!='codex' and config.get('selection')=='activity_expansion':
        activity_goals=[{'evidence_alternatives':[[{'source_id':ref['source_id'],'section_id':''} for ref in option] for option in fact.get('evidence_alternatives',[])]} for fact in criteria.get('facts',[])]
        minimum=goal_metrics(activity_goals,set())['minimum_required_sections']
        result['minimum_required_activities']=minimum
        result['seed_activity_capacity_limit']=config['top_k']
        result['goal_set_exceeds_capacity']=True if minimum is not None and minimum>config['top_k'] else None
    if envelope['operation']=='codex':
        result['section_capacity_limit']=None
        result['goal_set_exceeds_capacity']=None
        result['per_search_section_capacity_limit']=config['top_k']
    relevant={_key(ref) for ref in criteria.get('relevant_sections',[])}
    if criteria.get('relevance_labels_complete') and relevant:
        result['source_recall']=len(offered & relevant)/len(relevant)
        if offered:result['source_precision']=len(offered & relevant)/len(offered)
        if envelope['operation']!='codex':result['first_relevant_reciprocal_rank']=next((1/(i+1) for i,key in enumerate(seeds) if key in relevant),0)
    if calls:
        cumulative=set(); first_complete=None
        for i,call in enumerate(calls,1):
            if isinstance(call.get('result'),dict):
                cumulative.update(_key(hit['section']) for hit in call['result'].get('hits',[]))
            if result['target_goals'] and first_complete is None and goal_metrics(criteria.get('facts',[]),cumulative)['source_complete']:
                first_complete=i
        result['search_calls']=len(calls);result['first_complete_call']=first_complete
    return result


def _compact_assert(envelope, variables):
    if envelope.get('status')!='completed' or envelope.get('schema_version')!=1 or envelope['dataset_id']!=variables['dataset'] or envelope['case_id']!=variables['case_id']:
        raise ValueError('Compact replay does not belong to this completed test')
    criteria=load_case(variables['dataset'],variables['case_id'])
    if envelope['question']!=variables['question'] or envelope['question']!=criteria['question']:
        raise ValueError('Compact replay question differs from frozen input')
    record=envelope['record']
    if record.get('status')!='completed' or record.get('execution_mode')!='offline_projection_replay' or record.get('input_sha256')!=variables['input_sha256']:
        raise ValueError('Compact replay input provenance differs')
    full=record['full_response'];compact=record['compact_response']
    full_text=record['full_response_json'];compact_text=record['compact_response_json']
    if json.loads(full_text)!=full or json.loads(compact_text)!=compact:
        raise ValueError('Payload objects differ from measured UTF-8 text')
    if len(full_text.encode())!=record['full_payload_bytes'] or len(compact_text.encode())!=record['compact_payload_bytes']:
        raise ValueError('Payload byte measurements differ from exact serialized text')
    if compact.get('schema_version')!=2 or compact.get('payload_mode')!='compact' or compact.get('trace',{}).get('full_response_sha256')!=hashlib.sha256(full_text.encode()).hexdigest():
        raise ValueError('Compact audit digest/schema differs')
    for key in ('question','top_k','sources','as_of','manifest'):
        if compact.get(key)!=full.get(key):raise ValueError('Compact payload lost canonical metadata: '+key)
    if compact.get('hits')!=[{'section':hit['section']} for hit in full['hits']]:
        raise ValueError('Compact payload changed the offered source sections')
    if full.get('question')!=envelope['question'] or full.get('top_k')!=variables['top_k'] or full.get('as_of')!=criteria['as_of']:
        raise ValueError('Full payload differs from selected input/configuration')
    canonical=canonical_sources(variables['dataset'] in ('expanded-10-20261105','strengthened-10-20261106'))
    for hit in full['hits']:
        if canonical.get(_key(hit['section']))!=hit['section']:raise ValueError('Replay source text/date is not canonical')
    if full.get('hits')!=full.get('outcome',{}).get('hits') or full['outcome'].get('query')!=full['question']:
        raise ValueError('Replay-visible sources differ from full trace')
    native={'config':full['manifest']['config'],'dataset':{'corpus_sha256':criteria['corpus_sha256'],'as_of':criteria['as_of']},'outcome':full['outcome'],'preparation':full['preparation'],'request_filters':variables.get('filters',{'weekdays':[],'environment':None,'time':None})}
    native['request_filters']={'weekdays':[],'environment':None,'time':None,**native['request_filters']}
    _validate_local(native,variables,criteria,canonical)
    full_bytes=record['full_payload_bytes'];compact_bytes=record['compact_payload_bytes']
    reduction=1-compact_bytes/full_bytes
    metrics={'execution_integrity':True,'source_quality_applicable':False,'source_complete':None,'goal_coverage':None,'target_goals':0,'covered_goals':None,'answer_quality':'not_assessed','execution_mode':'offline_projection_replay','full_payload_bytes':full_bytes,'compact_payload_bytes':compact_bytes,'payload_reduction':reduction,'quality_comparison':'not_measured'}
    return {'pass':compact_bytes<full_bytes,'score':max(0,reduction),'reason':f'Exact source and metadata preservation; {full_bytes} to {compact_bytes} UTF-8 payload bytes. No answer-quality, token, latency or cost comparison.','namedScores':{'execution_integrity':1,'payload_reduction':reduction},'metadata':{'source_metrics':metrics}}


def get_assert(output, context):
    """Promptfoo's canonical assertion; failures are retained as real failures."""
    try:
        envelope=json.loads(output) if isinstance(output,str) else output
        if envelope.get('operation')=='compact':return _compact_assert(envelope,context.get('vars',{}))
        measurements=measure(envelope,context.get('vars',{}))
    except (ValueError,KeyError,TypeError,IndexError,json.JSONDecodeError) as error:
        return {'pass':False,'score':0,'reason':'Execution/source integrity failed: '+str(error),'namedScores':{'execution_integrity':0}}
    scores={'execution_integrity':1}
    for key in ('goal_coverage','source_complete','first_goal_support_reciprocal_rank','source_recall','source_precision','first_relevant_reciprocal_rank'):
        if measurements.get(key) is not None:scores[key]=float(measurements[key])
    if not measurements['source_quality_applicable']:
        return {'pass':True,'score':1,'reason':'Execution integrity passed; source quality not applicable: no labelled positive target. Answer behavior is not assessed.','namedScores':scores,'metadata':{'source_metrics':measurements}}
    complete=measurements['source_complete']
    reason=f"{measurements['covered_goals']}/{measurements['target_goals']} source-evidence goals complete"
    if measurements.get('goal_set_exceeds_capacity'):reason+='; required evidence exceeds this selection capacity'
    return {'pass':bool(complete),'score':measurements['goal_coverage'],'reason':reason,'namedScores':scores,'metadata':{'source_metrics':measurements}}


def get_model_assert(output, context):
    """Retain verified model defects as failed outcomes with unknown source quality."""
    envelope=json.loads(output) if isinstance(output,str) else output
    if not isinstance(envelope,dict) or envelope.get('status')=='completed':
        return get_assert(output,context)
    try:
        import importlib.util
        variables=context.get('vars',{})
        if context.get('test',{}).get('metadata',{}).get('suite')!='models':
            raise ValueError('Model failure classification requires the models suite')
        if envelope.get('schema_version')!=1 or envelope.get('operation')!='codex' or envelope.get('status')!='failed':
            raise ValueError('Unknown failed model envelope')
        criteria=load_case(variables['dataset'],variables['case_id'])
        if envelope.get('dataset_id')!=variables['dataset'] or envelope.get('case_id')!=variables['case_id'] or envelope.get('question')!=variables['question'] or variables['question']!=criteria['question']:
            raise ValueError('Failed model input differs from frozen case')
        evidence=envelope['model_failure_evidence'];record=envelope['record']
        digest=evidence['native_record_sha256']
        if not isinstance(digest,str) or len(digest)!=64 or any(c not in '0123456789abcdef' for c in digest):
            raise ValueError('Missing native failure evidence digest')
        if evidence['requested_model']!=variables['model'] or record.get('question')!=variables['question'] or record.get('trace_integrity_status')=='invalid':
            raise ValueError('Failed model identity/trace differs')
        spec=importlib.util.spec_from_file_location('coa_model_failure_guard',Path(__file__).with_name('model_control.py'))
        controls=importlib.util.module_from_spec(spec);spec.loader.exec_module(controls)
        failure=controls.classify_result({'live':record},evidence['returncode'])
        if failure not in ('model_output_invalid','model_contract_violation') or failure!=evidence['model_failure_class']:
            raise ValueError('Native failure is not a verified model outcome')
        metrics={'execution_integrity':True,'source_quality_applicable':False,'model_failure_class':failure,'source_complete':None,'goal_coverage':None,'covered_goals':None,'target_goals':None,'answer_quality':'not_assessed'}
        return {'pass':False,'score':0,'reason':'Verified model outcome: '+failure+'; source quality unassessed','namedScores':{'execution_integrity':1},'metadata':{'source_metrics':metrics}}
    except (ValueError,KeyError,TypeError,OSError,json.JSONDecodeError):
        return get_assert(output,context)
