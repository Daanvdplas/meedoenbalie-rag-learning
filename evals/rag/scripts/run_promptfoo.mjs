#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { mkdirSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomBytes, randomUUID } from 'node:crypto';
import { saveProjection } from './export_promptfoo_results.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)),'../../..');
const options = {};
const args = process.argv.slice(2);
const allowed = new Set(['suite','dataset','cases','methods','profiles','models','binary','model-cache','output-dir','ledger','max-generations','max-seconds','repeat','phase','approval']);
for (let i=0;i<args.length;i++) {
  const key = args[i].replace(/^--/,'');
  if (key==='help') { console.log('Gebruik: npm run eval:rag -- --suite garden|retrieval|quality|compact|all-local [--dataset ID --cases ID --methods METHOD --profiles PROFILE]\nCodex: npm run eval:rag:live -- --suite agentic|models|compact-live --ledger PAD --max-generations 8 --max-seconds 900 [--phase pilot|remaining|smoke --approval PAD --repeat 1]'); process.exit(0); }
  if (key==='live') { options.live=true; continue; }
  if (!allowed.has(key) || !args[i+1] || args[i+1].startsWith('--')) throw new Error('Onbekende of onvolledige optie: '+args[i]);
  options[key]=args[++i];
}
const suite = options.suite || 'retrieval';
const liveSuites = new Set(['agentic','models','compact-live']);
if (options.repeat && (!/^\d+$/.test(options.repeat) || Number(options.repeat)<1)) throw new Error('Repeat moet positief geheel zijn.');
if (liveSuites.has(suite) && !options.live) throw new Error('Deze suite vereist expliciet --live en een begrensd nieuw ledger.');
if (options.live && (!options.ledger || !options['max-generations'] || !options['max-seconds'])) throw new Error('Live vereist --ledger, --max-generations en --max-seconds.');
if (suite==='models' && !['pilot','remaining','smoke'].includes(options.phase)) throw new Error('Models vereist --phase pilot|remaining|smoke.');
if (options.phase && suite!=='models') throw new Error('--phase hoort uitsluitend bij models.');
if (suite==='models' && options.phase==='smoke' && (!options.cases || options.repeat!=='1')) throw new Error('Models-smoke vereist een expliciete caseselectie en --repeat 1.');
if (suite==='models' && options.phase!=='smoke' && (options.cases || options.models || (options.repeat && options.repeat!=='3'))) throw new Error('De volledige modelproef behoudt alle cases/modellen en drie herhalingen.');
if (options.live) {
  for (const [name,maximum] of [['max-generations',200],['max-seconds',14400]]) {
    if (!/^\d+$/.test(options[name]) || Number(options[name])<1 || Number(options[name])>maximum) throw new Error(`${name} moet een geheel getal van 1 tot ${maximum} zijn.`);
  }
  mkdirSync(dirname(resolve(options.ledger)),{recursive:true,mode:0o700});
}
const binary = resolve(options.binary || process.env.COA_RAG_BINARY || join(process.env.CARGO_TARGET_DIR || join(root,'target'),'debug','meedoen-retrieval'));
if (!existsSync(binary)) throw new Error('Bouw eerst cargo build --locked --features sqlite, of geef --binary mee.');
const runDir = resolve(options['output-dir'] || join(root,'evals/rag/results/local',new Date().toISOString().replace(/[:.]/g,'-')+'-'+randomBytes(3).toString('hex')));
mkdirSync(runDir,{recursive:true,mode:0o700});
const env = {...process.env,COA_RAG_BINARY:binary,COA_RAG_MODEL_CACHE:resolve(options['model-cache'] || process.env.COA_RAG_MODEL_CACHE || join(root,'.model-cache')),
  COA_PROMPTFOO_ARTIFACT_DIR:runDir,PROMPTFOO_CONFIG_DIR:join(runDir,'promptfoo-home'),PROMPTFOO_DISABLE_TELEMETRY:'1',PROMPTFOO_DISABLE_SHARING:'1'};
for (const key of ['OPENAI_API_KEY','COA_OPENAI_API_KEY_FILE','COA_RESPONSES_TEST_URL']) delete env[key];
if (options.repeat) env.COA_PROMPTFOO_REPEATS=options.repeat;
for (const name of ['COA_PROMPTFOO_PHASE','COA_PROMPTFOO_APPROVAL']) delete env[name];
if (options.phase) env.COA_PROMPTFOO_PHASE=options.phase;
if (options.approval) env.COA_PROMPTFOO_APPROVAL=resolve(options.approval);
for (const [flag,name] of Object.entries({dataset:'DATASET',cases:'CASES',methods:'METHODS',profiles:'PROFILES',models:'MODELS'})) if (options[flag]) env['COA_PROMPTFOO_'+name]=options[flag];
if (options.live) Object.assign(env,{COA_PROMPTFOO_ALLOW_CODEX:'1',COA_PROMPTFOO_LEDGER:resolve(options.ledger),COA_PROMPTFOO_MAX_GENERATIONS:options['max-generations'],COA_PROMPTFOO_MAX_SECONDS:options['max-seconds']});
else delete env.COA_PROMPTFOO_ALLOW_CODEX;
let interrupted = false;
async function execute(command, values, capture=false) {
  return new Promise((resolvePromise,reject)=>{
    const child = spawn(command,values,{cwd:root,env,stdio:capture?['ignore','pipe','inherit']:'inherit'});
    let output=''; if (capture) child.stdout.on('data',part=>output+=part);
    const stop=()=>{interrupted=true;child.kill('SIGINT');};process.once('SIGINT',stop);
    child.once('error',reject);child.once('close',code=>{process.removeListener('SIGINT',stop);resolvePromise({code:code??1,output});});
  });
}
const suites = suite==='all-local' ? ['garden','retrieval','quality','compact'] : [suite];
let exitCode=0;
for (const selected of suites) {
  if (interrupted) break;
  env.COA_PROMPTFOO_SUITE=selected;
  const config=join(root,'evals/rag/promptfoo/config.cjs');
  const output=join(runDir,selected+'.json');
  if (existsSync(output)) throw new Error('Een bestaand resultaat wordt niet overschreven: '+output);
  const plan=await execute(process.execPath,['-e','process.stdout.write(JSON.stringify(require(process.argv[1]).tests.map(test=>test.vars)))',config],true);
  if (plan.code!==0) throw new Error('De promptfoo-matrix kon niet worden geladen.');
  const tests=JSON.parse(plan.output);
  env.COA_PROMPTFOO_EXECUTION_ID=randomUUID();
  delete env.COA_PROMPTFOO_PREPARED;
  const inputs=join(runDir,selected+'-inputs.json');
  writeFileSync(inputs,JSON.stringify({schema_version:1,execution_id:env.COA_PROMPTFOO_EXECUTION_ID,tests:tests.map(vars=>({vars}))}),{mode:0o600});
  const prepare=join(root,'evals/rag/promptfoo/prepare.py');
  if (existsSync(prepare) && ['retrieval','quality'].includes(selected)) {
    env.COA_PROMPTFOO_PREPARED=join(runDir,selected+'-prepared.json');
    const result=await execute('python3',[prepare,'--input',inputs,'--output',env.COA_PROMPTFOO_PREPARED]);
    if (result.code!==0) throw new Error('Verse lokale retrievalvoorbereiding is mislukt.');
  }
  const command=[join(root,'node_modules/promptfoo/dist/src/main.js')];
  // Use the package's declared executable rather than guessing a generated entrypoint.
  const packageInfo=JSON.parse(readFileSync(join(root,'node_modules/promptfoo/package.json'),'utf8'));
  const entry=typeof packageInfo.bin==='string'?packageInfo.bin:packageInfo.bin.promptfoo;
  command[0]=join(root,'node_modules/promptfoo',entry);
  command.push('eval','-c',config,'--no-cache','--max-concurrency','1','--no-share','--no-table','-o',output);
  if (options.repeat) command.push('--repeat',options.repeat);
  const result=await execute(process.execPath,command);
  if (existsSync(output)) {
    const raw=JSON.parse(readFileSync(output,'utf8'));
    if (!raw.evalId || !Array.isArray(raw.results?.results)) throw new Error('Promptfoo heeft geen volledig leesbaar resultaat geschreven.');
    saveProjection(raw,join(root,'evals/rag/site/promptfoo-results.json'));
    writeFileSync(join(runDir,selected+'-summary.json'),JSON.stringify({evalId:raw.evalId,resultCount:raw.results.results.length,exitCode:result.code,rawFile:output}),{mode:0o600});
    console.log(JSON.stringify({suite:selected,evalId:raw.evalId,tests:raw.results.results.length,exitCode:result.code,output}));
  } else throw new Error('Promptfoo stopte zonder resultaatbestand.');
  // Assertion failures are real outcomes; preserve their raw output and continue other local suites.
  exitCode = Math.max(exitCode,result.code);
}
process.exitCode=interrupted?130:exitCode;
