'use strict';
(async function () {
  const byId = id => document.getElementById(id);
  function el(tag, text, className) {
    const element = document.createElement(tag);
    if (text !== undefined && text !== null) element.textContent = text;
    if (className) element.className = className;
    return element;
  }
  function date(value) {
    return new Intl.DateTimeFormat('nl-NL', {day: 'numeric', month: 'long', year: 'numeric', timeZone: 'UTC'}).format(new Date(value + 'T12:00:00Z'));
  }
  try {
    const response = await fetch('/assets/results.json', {cache: 'no-store'});
    if (!response.ok) throw new Error('De opgeslagen resultaten konden niet worden geladen.');
    const data = await response.json();
    if (data.schema_version !== 1 || data.questions.length !== 10 || data.documents.length !== 10 || data.models.length !== 2 || data.method !== 'dense') throw new Error('De resultaten horen niet bij deze meting.');
    function sourcePiece(key, className, rank) {
      const source = data.sources[key];
      if (!source) throw new Error('Een bronstukje ontbreekt.');
      const piece = el('article', null, className);
      piece.append(el('p', (rank ? rank + '. ' : '') + source.source_title + ' · ' + source.heading, 'source-title'));
      piece.append(el('p', 'Brondatum ' + date(source.source_date), 'source-date'));
      piece.append(el('p', source.text, 'source-text'));
      return piece;
    }
    data.models.forEach(model => {
      const row = el('tr');
      [model.label, model.complete + '/' + model.attempts, model.always_complete + '/10', model.actions].forEach(value => row.append(el('td', value)));
      byId('model-summary').append(row);
      const option = el('option', model.label); option.value = model.id; byId('model').append(option);
    });
    data.documents.forEach(document => {
      const details = el('details', null, 'activity'); details.name = 'activities';
      details.append(el('summary', document.title));
      const body = el('div', null, 'activity-body');
      body.append(el('p', 'Brondatum ' + date(document.date), 'source-date'));
      body.append(el('p', document.preamble));
      document.sections.forEach(key => {
        const source = data.sources[key];
        body.append(el('h3', source.heading), el('p', source.text));
      });
      details.append(body); byId('activities').append(details);
    });
    function closeRun(focus) {
      const button = document.querySelector('.trial-table button[aria-expanded="true"]');
      document.querySelectorAll('.run-panel').forEach(panel => panel.remove());
      document.querySelectorAll('.trial-table button').forEach(item => item.setAttribute('aria-expanded', 'false'));
      if (focus && button) button.focus({preventScroll: true});
    }
    function showRun(question, run, button, body) {
      const wasOpen = button.getAttribute('aria-expanded') === 'true';
      closeRun(false);
      if (wasOpen) return;
      button.setAttribute('aria-expanded', 'true');
      const panel = el('section', null, 'run-panel'); panel.id = button.getAttribute('aria-controls');
      const toolbar = el('div', null, 'run-toolbar');
      toolbar.append(el('h3', 'Herhaling ' + run.trial + ' · zoekacties'));
      const close = el('button', 'Sluit details'); close.type = 'button';
      close.addEventListener('click', () => closeRun(true)); toolbar.append(close); panel.append(toolbar);
      if (run.first_complete && run.first_complete < run.actions) panel.append(el('p', 'Verwachte bronnen compleet na zoekactie ' + run.first_complete + '; totaal ' + run.actions + ' uitgevoerd.', 'site-caption'));
      const expected = new Set(question.alternatives.flat());
      const steps = el('ol', null, 'search-steps');
      run.calls.forEach(call => {
        const step = el('li'); step.append(el('h4', 'Zoekactie ' + call.index), el('p', call.query, 'search-query'));
        const hits = el('div', null, 'hit-list');
        call.hits.forEach((key, index) => {
          const isExpected = expected.has(key);
          const hit = sourcePiece(key, 'hit' + (isExpected ? ' expected' : ''), index + 1);
          if (isExpected) hit.prepend(el('p', 'Verwacht bronstukje', 'expected-label'));
          hits.append(hit);
        });
        if (!call.hits.length) hits.append(el('p', 'Geen bronstukjes gevonden.', 'site-caption'));
        step.append(hits); steps.append(step);
      });
      panel.append(steps); body.append(panel);
    }
    function renderQuestions() {
      const openId = document.querySelector('.test-question[open]')?.dataset.case;
      closeRun(false); byId('test-questions').replaceChildren();
      const modelId = byId('model').value;
      data.questions.forEach(question => {
        const details = el('details', null, 'test-question'); details.name = 'test-questions'; details.dataset.case = question.id;
        details.open = question.id === openId;
        details.append(el('summary', question.question));
        const body = el('div', null, 'question-body');
        body.append(el('p', 'Deze bronstukjes verwachten we', 'expected-heading'));
        if (question.alternatives.length > 1) body.append(el('p', 'Eén volledige optie hieronder is voldoende. Tijd en locatie moeten bij dezelfde activiteit horen.', 'site-caption'));
        question.alternatives.forEach((option, index) => {
          const group = el('div', null, 'expected-group');
          if (question.alternatives.length > 1) group.append(el('p', 'Optie ' + (index + 1)));
          const pieces = el('div', null, 'expected-pieces');
          option.forEach(key => pieces.append(sourcePiece(key, 'expected-piece')));
          group.append(pieces); body.append(group);
        });
        const table = el('table', null, 'trial-table');
        const head = el('thead'), header = el('tr');
        ['Uitvoering', 'Zoekacties'].forEach(text => header.append(el('th', text))); head.append(header); table.append(head);
        const rows = el('tbody');
        const runs = question.runs.filter(run => run.model === modelId).sort((a, b) => a.trial - b.trial);
        if (runs.length !== 3) throw new Error('Een herhaling ontbreekt.');
        runs.forEach(run => {
          const row = el('tr'); row.append(el('td', 'Herhaling ' + run.trial));
          const cell = el('td');
          const label = run.actions + (run.actions === 1 ? ' zoekactie' : ' zoekacties') + (run.complete ? '' : ' · bronnen ontbreken');
          const button = el('button', label); button.type = 'button'; button.setAttribute('aria-expanded', 'false'); button.setAttribute('aria-controls', 'run-' + question.id + '-' + run.trial);
          button.addEventListener('click', () => showRun(question, run, button, body));
          cell.append(button); row.append(cell); rows.append(row);
        });
        table.append(rows); body.append(table); details.append(body); byId('test-questions').append(details);
      });
    }
    byId('model').addEventListener('change', renderQuestions);
    document.addEventListener('keydown', event => {
      if (event.key === 'Escape' && document.querySelector('.run-panel')) {closeRun(true); event.preventDefault();}
    });
    renderQuestions(); byId('load-status').hidden = true; byId('results-app').hidden = false;
  } catch (error) {
    byId('load-status').textContent = error.message || 'Resultaten konden niet worden geladen.';
    byId('load-status').className = 'error-message';
  }
})();
