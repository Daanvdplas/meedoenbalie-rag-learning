'use strict';
(async () => {
  const byId = id => document.getElementById(id);
  const node = (tag, text, className) => {
    const element = document.createElement(tag);
    if (text !== undefined && text !== null) element.textContent = text;
    if (className) element.className = className;
    return element;
  };
  const method = value => ({dense:'Op betekenis',bm25:'Op woorden',keyword_overlap:'Woordoverlap',hybrid_rrf:'Combinatie op volgorde',hybrid_weighted:'Gewogen combinatie'})[value] || value || '';
  const status = row => row.status === 'error' ? 'Uitvoeringsfout' : row.status === 'failed' ? 'Controle niet geslaagd' : 'Controles geslaagd';
  try {
    const response = await fetch('/api/evals/rag', {cache:'no-store'});
    if (!response.ok) throw new Error('De actuele evaluaties konden niet worden geladen.');
    const data = await response.json();
    if (data.schema_version !== 1 || data.source !== 'promptfoo' || !Array.isArray(data.evaluations)) throw new Error('Onbekend evaluatieformaat.');
    if (!data.evaluations.length) { byId('rag-eval-status').textContent = 'Er is nog geen promptfoo-uitvoering opgeslagen. De historische vergelijking staat hieronder.'; return; }
    byId('rag-eval-status').textContent = 'Uitgevoerd met promptfoo. Bronvolledigheid en technische controles staan los van antwoordkwaliteit.';
    byId('rag-eval-controls').hidden = false;
    for (const evaluation of data.evaluations) {
      const option = node('option',`${evaluation.suite} · ${evaluation.timestamp || evaluation.id}`);
      option.value = evaluation.id; byId('rag-evaluation').append(option);
    }
    function render() {
      const evaluation = data.evaluations.find(item=>item.id===byId('rag-evaluation').value) || data.evaluations[0];
      byId('rag-eval-run-id').textContent = 'Uitvoering: ' + evaluation.id;
      const filter = byId('rag-dataset').value;
      const rows = evaluation.rows.filter(row=>!filter || row.dataset===filter);
      const applicable = rows.filter(row=>row.source_complete!==null);
      const complete = applicable.filter(row=>row.source_complete).length;
      byId('rag-eval-summary').textContent = `${rows.length} tests · ${rows.filter(row=>row.status==='failed').length} controles niet geslaagd · ${rows.filter(row=>row.status==='error').length} uitvoeringsfouten · complete bronsets: ${complete}/${applicable.length}. Zonder bewijsdoel volgt geen bronkwaliteitsoordeel.`;
      byId('rag-eval-cases').replaceChildren();
      const groups = new Map();
      for (const row of rows) {
        const key = row.dataset + ':' + row.case_id;
        if (!groups.has(key)) groups.set(key,[]);
        groups.get(key).push(row);
      }
      for (const [key, runs] of groups) {
        const details = node('details',null,'test-question promptfoo-case'); details.name = 'promptfoo-cases'; details.dataset.case = key;
        details.append(node('summary',`${runs[0].case_id} · ${runs[0].question}`));
        const body = node('div',null,'question-body');
        body.append(node('p',runs[0].dataset,'site-caption'));
        for (const row of runs) {
          const run = node('details',null,'promptfoo-run'); run.name = 'promptfoo-runs-' + key;
          const label = [method(row.configuration.method),row.configuration.profile,row.configuration.model,row.configuration.payload_mode].filter(Boolean).join(' · ') || row.provider;
          run.append(node('summary',label + ' · ' + status(row)));
          const inner = node('div',null,'question-body');
          inner.append(node('p',row.reason,row.status==='error' || row.status==='failed' ? 'error-message' : 'site-caption'));
          inner.append(node('p',row.source_complete===null ? 'Bronkwaliteit: niet van toepassing of niet beoordeeld.' : `Bewijsdoelen: ${row.metrics.covered_goals}/${row.metrics.target_goals}. Bronset ${row.source_complete ? 'compleet' : 'onvolledig'}.`));
          const metricLabels = {goal_coverage:'Gedeeltelijke bewijsdekking',source_precision:'Relevante gevonden secties',source_recall:'Gevonden relevante secties',first_goal_support_reciprocal_rank:'Rang eerste bewijsstukje',first_relevant_reciprocal_rank:'Rang eerste relevante sectie'};
          const metricText = Object.entries(metricLabels).filter(([key])=>Number.isFinite(row.metrics[key])).map(([key,label])=>label+': '+Number(row.metrics[key]).toLocaleString('nl-NL',{maximumFractionDigits:3})).join(' · ');
          if (metricText) inner.append(node('p',metricText,'site-caption'));
          if (Number.isFinite(row.metrics.minimum_required_activities)) inner.append(node('p',`Benodigde activiteiten: ${row.metrics.minimum_required_activities}; maximaal ${row.metrics.seed_activity_capacity_limit} startactiviteiten geselecteerd.`,'site-caption'));
          if (row.metrics.goal_set_exceeds_capacity) inner.append(node('p',`De benodigde bronset past niet in één zoekselectie: minimaal ${row.metrics.minimum_required_sections} secties, maximaal ${row.metrics.section_capacity_limit} geselecteerd.`));
          const checks = node('ul');
          row.assertions.forEach(check=>checks.append(node('li',(check.pass?'Geslaagd: ':'Niet geslaagd: ') + check.reason)));
          inner.append(checks);
          if (Number.isFinite(row.metrics.full_payload_bytes ?? row.metrics.full_bytes)) inner.append(node('p',`Offline payloadprojectie: ${row.metrics.full_payload_bytes ?? row.metrics.full_bytes} → ${row.metrics.compact_payload_bytes ?? row.metrics.compact_bytes} bytes. Dit meet geen modelkosten of snelheidswinst.`,'site-caption'));
          if (row.answer) inner.append(node('h3','Antwoord (inhoudelijk niet beoordeeld)'),node('p',row.answer));
          row.calls.forEach(call=>{
            const step = node('section',null,'search-action');
            step.append(node('h3','Zoekactie ' + call.index),node('p',call.query,'search-query'));
            call.hits.forEach(sourceKey=>{
              const source = data.sources[sourceKey];
              if (!source) return;
              const piece = node('article',null,'hit');
              piece.append(node('h4',source.source_title + ' · ' + source.heading),node('p',source.source_id + ':' + source.id + ' · brondatum ' + source.source_date,'source-date'),node('blockquote',source.text,'source-text'));
              step.append(piece);
            });
            if (!call.hits.length) step.append(node('p','Geen bronpassages teruggegeven.'));
            inner.append(step);
          });
          if (row.original_evaluation_id) inner.append(node('p','Herbeoordeling van ' + row.original_evaluation_id + (row.original_timestamp ? ' · oorspronkelijke uitvoering ' + row.original_timestamp : '') + '. Geen nieuwe modelaanroep.','site-caption'));
          const metrics = [row.retrieval_ms===null ? null : `Lokale zoekberekening: ${row.retrieval_ms.toFixed(2)} ms`,row.native_elapsed_ms===null ? null : `Volledige native sessie: ${row.native_elapsed_ms} ms`,row.promptfoo_ms===null ? null : `Testdoorlooptijd: ${row.promptfoo_ms} ms`].filter(Boolean).join(' · ');
          inner.append(node('p',metrics,'site-caption'),node('p','Herkomst: ' + row.execution_origin + ' · ' + row.provider,'site-caption'),node('p','Resultaat: ' + row.id + (row.native_run_id ? ' · native run: ' + row.native_run_id : ''),'site-caption'));
          run.append(inner); body.append(run);
        }
        details.append(body); byId('rag-eval-cases').append(details);
      }
    }
    function selectEvaluation() {
      const evaluation = data.evaluations.find(item=>item.id===byId('rag-evaluation').value) || data.evaluations[0];
      byId('rag-dataset').replaceChildren(node('option','Alle vraagsets'));
      byId('rag-dataset').firstChild.value = '';
      [...new Set(evaluation.rows.map(row=>row.dataset))].forEach(dataset=>{const option=node('option',dataset);option.value=dataset;byId('rag-dataset').append(option);});
      render();
    }
    byId('rag-evaluation').addEventListener('change',selectEvaluation);
    byId('rag-dataset').addEventListener('change',render);
    selectEvaluation();
  } catch (error) { byId('rag-eval-status').textContent = error.message || 'De actuele evaluaties zijn niet beschikbaar.'; }
})();
