import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, renameSync, mkdirSync, existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const text = value => typeof value === 'string' ? value : '';
const publicReason = value => /\/Users\/|\/private\/|access_token|OPENAI_API_KEY|reasoning\.encrypted/i.test(text(value)) ? 'Details staan alleen in het lokale rapport.' : text(value).slice(0, 2000);
const fields = (object, names) => Object.fromEntries(names.filter(name => object?.[name] === null || ['string','number','boolean'].includes(typeof object?.[name])).map(name => [name, object[name]]));

export function project(input) {
  const results = input.results?.results;
  if (typeof input.evalId !== 'string' || !Array.isArray(results)) throw new Error('Geen ondersteund promptfoo-resultaat.');
  const sources = {};
  function sourceKey(hit) {
    const section = hit?.section || hit;
    if (!section || !['source_id','source_date','source_title','id','heading','text'].every(key => typeof section[key] === 'string')) return null;
    const safe = fields(section, ['source_id','source_date','source_title','id','heading','text']);
    const key = `${safe.source_id}:${safe.id}:${createHash('sha256').update(JSON.stringify(safe)).digest('hex').slice(0,12)}`;
    sources[key] = safe;
    return key;
  }
  const rows = results.map((result, index) => {
    const vars = result.testCase?.vars || result.vars || {};
    let envelope = result.response?.output;
    try { if (typeof envelope === 'string') envelope = JSON.parse(envelope); } catch { envelope = null; }
    const record = envelope?.record || {};
    const live = record.live || record;
    const run = live.run || record.run;
    const grading = result.gradingResult || {};
    const components = grading.componentResults || [];
    const metrics = components.find(item => item.metadata?.source_metrics)?.metadata.source_metrics || grading.metadata?.source_metrics || {};
    const modelFailure = metrics.execution_integrity === true && ['model_output_invalid','model_contract_violation'].includes(metrics.model_failure_class);
    const declaredStatuses = [envelope?.status,record.status,record.execution_status,live.status,run?.status].filter(value=>value!==undefined);
    const runtimeError = Boolean(result.response?.error || !envelope || (envelope.status !== 'completed' && !modelFailure) || (declaredStatuses.some(value=>value!=='completed') && !modelFailure) || metrics.execution_integrity === false || metrics.execution_integrity === 0 || grading.namedScores?.execution_integrity === 0 || components.some(item => item.metadata?.graderError || item.namedScores?.execution_integrity === 0));
    const applicable = !runtimeError && metrics.source_quality_applicable === true && Number.isInteger(metrics.target_goals) && metrics.target_goals > 0;
    const sourceComplete = applicable ? metrics.covered_goals === metrics.target_goals : null;
    let calls = [];
    if (Array.isArray(run?.tool_calls)) calls = run.tool_calls.map((call, callIndex) => ({
      index:callIndex+1, query:text(call.query), hits:(call.result?.hits || call.result?.sections || []).map(sourceKey).filter(Boolean), status:text(call.status)
    }));
    else if (record.outcome || Array.isArray(record.hits)) calls = [{index:1,query:text(record.question || vars.question),hits:(record.outcome?.hits || record.hits || []).map(sourceKey).filter(Boolean),status:'completed'}];
    else if (Array.isArray(record.full_response?.hits)) calls = [{index:1,query:text(record.full_response.question || vars.question),hits:record.full_response.hits.map(sourceKey).filter(Boolean),status:'completed'}];
    const checks = components.map(item => ({pass:item.pass === true, score:typeof item.score === 'number' ? item.score : null, reason:publicReason(item.reason)}));
    const configuration = fields(vars,['mode','model','method','profile','top_k','candidate_k','context_token_limit','selection','apply_filters','mmr','payload_mode']);
    if (!['codex','compact-live'].includes(vars.mode)) delete configuration.model;
    return {
      id:text(result.id) || `${input.evalId}-${index+1}`, native_run_id:text(record.run_id || live.run_id || run?.id),
      dataset:text(vars.dataset || vars.dataset_id || envelope?.dataset_id), case_id:text(vars.case_id || envelope?.case_id),
      question:text(vars.question || envelope?.question), suite:text(vars.suite || result.testCase?.metadata?.suite || input.config?.description || 'rag'),
      provider:publicReason(result.provider?.label) || [text(vars.model),text(vars.method),text(vars.profile)].filter(Boolean).join(' · ') || 'Lokale uitvoering',
      configuration,
      status:runtimeError ? 'error' : modelFailure ? 'failed' : grading.pass === true ? 'passed' : 'failed',
      reason:runtimeError ? 'De uitvoering kon niet worden afgerond; bekijk het lokale rapport.' : publicReason(grading.reason),
      assertions:checks, source_complete:sourceComplete,
      metrics:fields(metrics,['model_failure_class','target_goals','covered_goals','goal_coverage','offered_sections','unknown_obligations','minimum_required_sections','minimum_required_activities','seed_activity_capacity_limit','section_capacity_limit','goal_set_exceeds_capacity','source_quality_applicable','first_complete_call','first_complete_call_index','search_calls','source_precision','source_recall','first_goal_support_reciprocal_rank','first_relevant_reciprocal_rank','full_bytes','compact_bytes','full_payload_bytes','compact_payload_bytes']),
      promptfoo_ms:typeof result.latencyMs === 'number' ? result.latencyMs : null,
      retrieval_ms:typeof record.timings?.retrieval_ms === 'number' ? record.timings.retrieval_ms : null,
      native_elapsed_ms:typeof live.elapsed_ms === 'number' ? live.elapsed_ms : null,
      original_evaluation_id:text(result.testCase?.metadata?.original_evaluation_id), original_timestamp:text(result.testCase?.metadata?.original_timestamp),
      execution_origin:text(result.testCase?.metadata?.execution_origin || envelope?.execution_origin || envelope?.execution_mode || result.response?.metadata?.execution_mode || result.response?.metadata?.execution_origin || 'native'),
      answer:typeof run?.answer?.text === 'string' ? run.answer.text : null, calls
    };
  });
  return {schema_version:1,source:'promptfoo',answer_quality:'not_assessed',sources,evaluations:[{
    id:input.evalId,timestamp:text(input.results.timestamp),suite:rows[0]?.suite || 'rag',rows,
    source_completion:{numerator:rows.filter(row=>row.source_complete===true).length,denominator:rows.filter(row=>row.source_complete!==null).length},
    statuses:{passed:rows.filter(row=>row.status==='passed').length,failed:rows.filter(row=>row.status==='failed').length,error:rows.filter(row=>row.status==='error').length}
  }]};
}

export function saveProjection(input, output) {
  const fresh = project(input);
  if (existsSync(output)) {
    const old = JSON.parse(readFileSync(output,'utf8'));
    if (old.schema_version === 1 && old.source === 'promptfoo' && Array.isArray(old.evaluations)) {
      fresh.evaluations = [...fresh.evaluations,...old.evaluations.filter(item=>item.id!==input.evalId)].slice(0,20);
      fresh.sources = {...old.sources,...fresh.sources};
    }
  }
  // Keep only sources reachable from retained evaluations.
  const used = new Set(fresh.evaluations.flatMap(item=>item.rows.flatMap(row=>row.calls.flatMap(call=>call.hits))));
  fresh.sources = Object.fromEntries(Object.entries(fresh.sources).filter(([key])=>used.has(key)));
  mkdirSync(dirname(output),{recursive:true});
  writeFileSync(output+'.tmp',JSON.stringify(fresh,null,2)+'\n',{mode:0o644});
  renameSync(output+'.tmp',output);
  return fresh;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)),'../../..');
  const input = process.argv[2];
  if (!input) throw new Error('Gebruik: node export_promptfoo_results.mjs <promptfoo.json> [public.json]');
  const output = process.argv[3] || resolve(root,'evals/rag/site/promptfoo-results.json');
  const result = saveProjection(JSON.parse(readFileSync(input,'utf8')),output);
  console.log(JSON.stringify({evaluations:result.evaluations.length,output}));
}
