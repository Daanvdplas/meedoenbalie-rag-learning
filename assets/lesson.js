document.querySelectorAll('[data-walkthrough]').forEach((walkthrough) => {
  const panels = [...walkthrough.querySelectorAll('[data-panel]')];
  const steps = [...walkthrough.querySelectorAll('[data-step]')];
  const diagramNodes = [...document.querySelectorAll('[data-map-step]')];
  const previous = walkthrough.querySelector('[data-previous]');
  const next = walkthrough.querySelector('[data-next]');
  const status = walkthrough.querySelector('[data-status]');
  let current = 0;

  function showStep(index) {
    current = Math.min(Math.max(index, 0), panels.length - 1);
    panels.forEach((panel, position) => { panel.hidden = position !== current; });
    steps.forEach((step) => { step.setAttribute('aria-pressed', String(Number(step.dataset.step) === current)); });
    diagramNodes.forEach((node) => { node.setAttribute('aria-pressed', String(Number(node.dataset.mapStep) === current)); });
    previous.disabled = current === 0;
    next.disabled = current === panels.length - 1;
    status.textContent = `Step ${current + 1} of ${panels.length}`;
  }

  steps.forEach((step) => step.addEventListener('click', () => showStep(Number(step.dataset.step))));
  diagramNodes.forEach((node) => node.addEventListener('click', () => showStep(Number(node.dataset.mapStep))));
  previous.addEventListener('click', () => showStep(current - 1));
  next.addEventListener('click', () => showStep(current + 1));
  showStep(0);
});

document.querySelectorAll('[data-quiz]').forEach((quiz) => {
  const feedback = quiz.querySelector('[data-feedback-output]');
  quiz.addEventListener('submit', (event) => {
    event.preventDefault();
    const choice = quiz.querySelector('input:checked');
    feedback.hidden = false;
    if (!choice) {
      feedback.textContent = 'Choose an answer first. You can change it and try again.';
      return;
    }
    feedback.textContent = choice.dataset.feedback;
  });
});
