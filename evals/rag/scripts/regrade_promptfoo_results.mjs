#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'../../..');
const runtimeVariables=['dataset','case_id','question','mode','method','profile','top_k','candidate_k','context_token_limit','selection','apply_filters','mmr','rrf_k','vector_weight','mmr_lambda','model','filters','payload_mode','replay_path','input_sha256'];

export function buildRegradeConfig(raw, rawSha256) {
  if (typeof raw.evalId!=='string' || !Array.isArray(raw.results?.results)) throw new Error('Expected a saved promptfoo evaluation.');
  return {
    description:'COA RAG / recorded execution regrading / '+raw.evalId,
    basePath:join(root,'evals/rag/promptfoo'),
    prompts:['{{recorded_output}}'],providers:['echo'],
    tests:raw.results.results.map(row=>{
      const output=row.response?.output;
      if (output===undefined || output===null || row.response?.error) throw new Error('A recorded output is missing or failed; it cannot be regraded as completed evidence.');
      const vars=row.testCase?.vars || {};
      return {
        vars:{...Object.fromEntries(runtimeVariables.filter(key=>key in vars).map(key=>[key,vars[key]])),recorded_output:typeof output==='string'?output:JSON.stringify(output)},
        metadata:{suite:row.testCase?.metadata?.suite || 'rag',native_record_sha256:row.response?.metadata?.native_record_sha256,execution_origin:'recorded_execution_regrading',original_evaluation_id:raw.evalId,original_result_id:row.id,original_timestamp:raw.results.timestamp,original_raw_sha256:rawSha256,answer_quality:'not_assessed'},
        options:{repeat:1,disableVarExpansion:true},
        assert:[{type:'python',value:'file://'+join(root,'evals/rag/promptfoo/assertions.py')+':get_assert'}]
      };
    }),
    evaluateOptions:{maxConcurrency:1,maxRetries:0,cache:false}
  };
}

if (process.argv[1] && resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const [input, outputDirectory]=process.argv.slice(2);
  if (!input || !outputDirectory) throw new Error('Gebruik: node regrade_promptfoo_results.mjs <origineel.json> <nieuwe-runmap>');
  const runDir=resolve(outputDirectory);
  if (existsSync(runDir)) throw new Error('Kies een nieuwe map; oorspronkelijke resultaten worden niet overschreven.');
  mkdirSync(runDir,{recursive:true,mode:0o700});
  const bytes=readFileSync(input),digest=createHash('sha256').update(bytes).digest('hex');
  let recorded=JSON.parse(bytes);
  const hasCodex=recorded.results?.results?.some(row=>{
    let output=row.response?.output;
    if (typeof output==='string') output=JSON.parse(output);
    return output?.operation==='codex';
  });
  if (hasCodex) {
    const enriched=join(runDir,'enriched-records.json');
    const projection=spawnSync('python3',[join(root,'evals/rag/promptfoo/regrade_inputs.py'),'--input',resolve(input),'--output',enriched],{cwd:root,stdio:'inherit'});
    if (projection.status!==0) throw new Error('Oorspronkelijke native/auditbinding kon niet worden bevestigd.');
    recorded=JSON.parse(readFileSync(enriched));
  }
  const config=buildRegradeConfig(recorded,digest);
  const configPath=join(runDir,'regrade.config.json'),output=join(runDir,'results.json');
  writeFileSync(configPath,JSON.stringify(config),{mode:0o600});
  const env={...process.env,PROMPTFOO_CONFIG_DIR:join(runDir,'promptfoo-home'),PROMPTFOO_DISABLE_TELEMETRY:'1',PROMPTFOO_DISABLE_SHARING:'1'};
  for (const key of ['OPENAI_API_KEY','COA_OPENAI_API_KEY_FILE','COA_PROMPTFOO_ALLOW_CODEX','OPENAI_BASE_URL','OPENAI_API_BASE']) delete env[key];
  const packageInfo=JSON.parse(readFileSync(join(root,'node_modules/promptfoo/package.json')));
  const entry=typeof packageInfo.bin==='string'?packageInfo.bin:packageInfo.bin.promptfoo;
  const result=spawnSync(process.execPath,[join(root,'node_modules/promptfoo',entry),'eval','-c',configPath,'--no-cache','--max-concurrency','1','--no-share','--no-table','-o',output],{cwd:root,env,stdio:'inherit'});
  if (!existsSync(output)) throw new Error('Promptfoo heeft geen herbeoordeling geschreven.');
  const graded=JSON.parse(readFileSync(output));
  writeFileSync(join(runDir,'provenance.json'),JSON.stringify({schema_version:1,operation:'assertion_regrading_only',original_evaluation_id:JSON.parse(bytes).evalId,original_raw_sha256:digest,grading_id:graded.evalId,model_calls:0,original_result_count:config.tests.length},null,2)+'\n',{mode:0o600});
  process.exitCode=result.status??1;
}
