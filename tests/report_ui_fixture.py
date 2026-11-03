"""Build explicitly synthetic browser fixtures; no model, labels or timing claims.

python3 tests/report_ui_fixture.py /tmp/coa-report-ui
python3 -m http.server 8794 --bind 127.0.0.1 --directory /tmp/coa-report-ui
Run tests/report_ui.browser.js with dev-browser against that fixture server.
"""

import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
output = Path(sys.argv[1])
output.mkdir(parents=True, exist_ok=True)
unsafe = "</script><script>window.reportInjected=true</script>&\u2028"
section = dict(
    source_id="ACT-TUIN-001",
    id="S2",
    source_title="Tuinieren",
    heading="Wanneer",
    source_date="2026-10-20",
    text="Dinsdag 10–12. " + unsafe,
)


def row(
    case_id,
    covered,
    target,
    *,
    dataset="legacy",
    scenario="baseline",
    top_k=3,
    minimum=2,
    action="answer_with_sources",
    minimum_activities=None,
):
    config = dict(
        method="dense",
        top_k=top_k,
        candidate_k=30,
        context_token_limit=1200,
        selection="sections" if scenario == "baseline" else "activity_expansion",
        apply_filters=scenario == "filtered_expansion",
        mmr=None,
    )
    run_id = f"{dataset}-{case_id}-{scenario}"
    facts = [
        dict(
            criterion=dict(
                id=f"F{i}",
                description=f"Verwacht bewijsdoel {i}",
                evidence_alternatives=[
                    [dict(source_id="ACT-TUIN-001", section_id=f"S{i + 2}")]
                ],
            ),
            covered=i < covered,
            missing_by_alternative=[]
            if i < covered
            else [[dict(source_id="ACT-TUIN-001", section_id=f"S{i + 2}")]],
        )
        for i in range(target)
    ]
    hits = [
        dict(section={**section, "id": section_id}, inclusion="search", scores=None)
        for section_id in ["S1"] + [f"S{i + 2}" for i in range(covered)]
    ]
    assessment = dict(
        kind="deterministic",
        deterministic=dict(
            expected_facts=facts,
            required_unknowns=["Actuele plaatsen zijn onbekend"] if not target else [],
            conditions=[],
            hard_checks=[
                dict(id="canonical_evidence", passed=True, detail="Brontekst intact")
            ],
            metrics=[],
        ),
    )
    return dict(
        record=dict(
            run_id=run_id,
            case_id=case_id,
            question=(
                "Zijn er plaatsen? " if not target else "Waar en wanneer tuinieren? "
            )
            + unsafe,
            kind="missing_information" if not target else "multiple_facts",
            execution_status="completed",
            execution_kind="fixture",
            config_id=f"dense_documents_sections_{scenario}",
            config=dict(profile="documents_sections", retrieval=config),
            error=None,
            outcome=dict(hits=hits, estimated_context_tokens=40, filters=None),
            preparation=dict(
                model_load_ms=None,
                document_embedding_ms=None,
                storage_sync_ms=None,
                storage_load_ms=None,
                preparation_id=None,
                document_cache_hits=0,
                document_cache_misses=0,
                actual_profile="documents_sections",
            ),
            timings=dict(
                query_embedding_ms=None,
                query_embedding_cache_hit=None,
                retrieval_ms=None,
            ),
            dataset=dict(corpus_sha256="source-sha"),
            request_filters=None,
        ),
        record_sha256=(str(len(case_id)) * 64),
        assessments=[assessment],
        task_metrics=dict(
            complete_goal_set=None if not target else covered == target,
            covered_goals=None if not target else covered,
            target_goals=target,
            partial_goal_coverage=None if not target else covered / target,
            first_goal_support_reciprocal_rank=None if not target else 0.5,
            minimum_required_sections=minimum,
            section_capacity_limit=top_k if scenario == "baseline" else None,
            minimum_required_activities=minimum_activities,
            seed_activity_capacity_limit=top_k if scenario != "baseline" else None,
            goal_set_exceeds_capacity=(
                minimum > top_k
                if scenario == "baseline"
                else minimum_activities > top_k
                if minimum_activities is not None
                else None
            ),
            unknown_obligations=1 if not target else 0,
            answer_review="not_generated",
            not_applicable_reason="No labelled source target" if not target else None,
        ),
        dataset_info=dict(
            id=dataset,
            provenance="Agentreferentie; niet door mensen beoordeeld",
            limitation="Dezelfde vijf bekende documenten; geen bewijs voor nieuwe documenten",
            freeze_sha256="frozen-sha" if dataset != "legacy" else None,
        ),
        scenario=dict(
            id=scenario,
            label={
                "baseline": "Basis · 3 passages",
                "activity_expansion": "Hele activiteit",
                "filtered_expansion": "Hele activiteit + vraagfilters",
            }[scenario],
            filter_effect="no_filters_requested"
            if scenario == "filtered_expansion"
            else "disabled",
            query_inputs_sha256=None,
        ),
        reference_metadata=dict(
            expected_action=action,
            fact_roles={"F0": "unknown"} if target else {},
            origin="Agentreferentie, apart gecontroleerd",
        ),
    )


rows = [
    row("T01", 2, 2),
    row("T02", 0, 2),
    row("T05", 1, 4, minimum=4),
    row("P05", 0, 0, minimum=0, action="state_unknown"),
    row("NO_MATCH", 0, 2, action="no_match_in_known_corpus"),
    row("CLARIFY", 0, 0, minimum=0, action="ask_which_activity"),
    row("N01", 2, 2, dataset="independent-20261103"),
    row("N01", 2, 2, dataset="independent-20261103", scenario="activity_expansion"),
    row("N01", 2, 2, dataset="independent-20261103", scenario="filtered_expansion"),
    row(
        "N02",
        1,
        5,
        dataset="independent-20261103",
        scenario="activity_expansion",
        minimum=5,
        minimum_activities=5,
    ),
]
data = dict(
    schema_version=1,
    artifact_id="qa-report-ui-only-v1",
    assessor_version="retrieval-deterministic-v2",
    record_hash_policy="exact-jsonl-bytes-v1",
    task_metrics_version="retrieval-task-metrics-v1",
    shared_preparations=[],
    rows=rows,
)
html = (
    (ROOT / "src/retrieval_lab/report.html")
    .read_text()
    .replace("__REPORT_STYLE__", (ROOT / "src/retrieval_lab/report.css").read_text())
    .replace("__REPORT_SCRIPT__", (ROOT / "src/retrieval_lab/report.js").read_text())
)
payload = (
    json.dumps(data, ensure_ascii=True)
    .replace("&", "\\u0026")
    .replace("<", "\\u003c")
    .replace(">", "\\u003e")
)
(output / "fixture.html").write_text(html.replace("__REPORT_DATA__", payload))
(output / "empty.html").write_text(
    html.replace("__REPORT_DATA__", json.dumps({**data, "rows": []}))
)
print(output)
