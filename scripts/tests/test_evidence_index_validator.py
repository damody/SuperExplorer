from __future__ import annotations

import hashlib
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "evidence_index_validator.py"
SPEC = importlib.util.spec_from_file_location("evidence_index_validator", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def leaf_report_bytes(task_id: str) -> bytes:
    report = {
        "schema_version": 1,
        "task_id": task_id,
        "actual": "passed",
        "exit_code": 0,
        "environment": {"uitest_executed": "false"},
    }
    return (json.dumps(report, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")


def record(task_id: str = "1.1.1", **overrides: object) -> dict[str, object]:
    digest = hashlib.sha256(leaf_report_bytes(task_id)).hexdigest()
    value: dict[str, object] = {
        "schema_version": 1,
        "event_id": f"event-{task_id}",
        "previous_event_sha256": None,
        "previous_index_sha256": None,
        "task_id": task_id,
        "record_kind": "leaf-result",
        "priority": "P0",
        "release_blocking": True,
        "mandatory": True,
        "status": "passed",
        "gate_id": f"gate-{task_id}",
        "procedure_kind": "command",
        "subcheck_key": f"command-{task_id}",
        "artifact_or_command": "python -m unittest scripts.tests.test_evidence_index_validator",
        "cwd": ".",
        "environment": {"CARGO_NET_OFFLINE": "true", "uitest_executed": "false"},
        "expected_exit_and_artifacts": "exit 0; deterministic local report hash",
        "expected": "validator passes",
        "actual": "passed: validator",
        "exit_code_or_reviewer": 0,
        "sha256": digest,
        "local_result_path": f"{task_id}/result.json",
        "immutable_locator": None,
        "artifact_manifest_sha256": None,
        "artifact_manifest_locator": None,
        "retention_policy": "local-rerunnable-result",
        "related_gates": [f"gate-{task_id}"],
        "adjustment_id": "A-001",
        "timestamp": "2026-08-03T00:00:00Z",
        "evidence_scope": "production",
        "depends_on": [],
    }
    value.update(overrides)
    return value


def retained_record(task_id: str = "1.1.1", **overrides: object) -> dict[str, object]:
    digest = "a" * 64
    value = record(
        task_id,
        record_kind="retained-bundle",
        local_result_path=None,
        sha256=digest,
        immutable_locator=f"release://local/evidence-{task_id}#sha256={digest}",
        artifact_manifest_sha256="b" * 64,
        artifact_manifest_locator=f"release://local/evidence-manifest-{task_id}#sha256={'b' * 64}",
        retention_policy="signed-release-evidence-bundle",
    )
    value.update(overrides)
    return value


def link_events(*events: dict[str, object]) -> list[dict[str, object]]:
    prior_index_hash: str | None = None
    prior_task_hashes: dict[str, str] = {}
    linked: list[dict[str, object]] = []
    for number, event in enumerate(events, start=1):
        event["event_id"] = f"event-{event['task_id']}-{number}"
        event["previous_index_sha256"] = prior_index_hash
        event["previous_event_sha256"] = prior_task_hashes.get(str(event["task_id"]))
        event_hash = MODULE.canonical_event_sha256(event)
        prior_index_hash = event_hash
        prior_task_hashes[str(event["task_id"])] = event_hash
        linked.append(event)
    return linked


class EvidenceIndexValidatorTests(unittest.TestCase):
    def validate(self, *records: dict[str, object], verify: bool = False, locator_map: dict[str, str] | None = None, tasks: str | None = None, require_complete: bool = False, policy: dict[str, object] | None = None, lineage_mapping: dict[str, object] | None = None, closure_tasks: set[str] | None = None, closure_kind: str = "leaf") -> list[object]:
        with tempfile.TemporaryDirectory() as temporary:
            index = Path(temporary) / "evidence-index.jsonl"
            index.write_text("\n".join(json.dumps(item) for item in records), encoding="utf-8")
            tasks_path = None
            if tasks is not None:
                tasks_path = Path(temporary) / "tasks.md"
                tasks_path.write_text(tasks, encoding="utf-8")
            policy_path = None
            if policy is not None:
                policy_path = Path(temporary) / "policy.json"
                policy_path.write_text(json.dumps(policy), encoding="utf-8")
            mapping_path = None
            if lineage_mapping is not None:
                mapping_path = Path(temporary) / "legacy-lineage-map.json"
                mapping_path.write_text(json.dumps(lineage_mapping), encoding="utf-8")
            result_root = Path(temporary) / "results"
            for item in records:
                if item.get("record_kind") != "leaf-result":
                    continue
                result_path = result_root / str(item["task_id"]) / "result.json"
                result_path.parent.mkdir(parents=True, exist_ok=True)
                result_path.write_bytes(leaf_report_bytes(str(item["task_id"])))
            return MODULE.validate_index(index, verify_artifacts=verify, locator_map=locator_map, tasks_path=tasks_path, require_complete=require_complete, closure_policy_path=policy_path, lineage_mapping_path=mapping_path, closure_task_ids=closure_tasks, closure_kind=closure_kind, local_result_root=result_root)

    def messages(self, *records: dict[str, object]) -> str:
        return "\n".join(str(issue) for issue in self.validate(*records))

    def test_accepts_one_terminal_l3_record(self) -> None:
        self.assertEqual(self.validate(record()), [])

    def test_leaf_result_requires_no_release_bundle_locator(self) -> None:
        self.assertEqual(self.validate(record()), [])
        messages = self.messages(record(immutable_locator="release://local/unwanted#sha256=" + "a" * 64))
        self.assertIn("leaf-result must not require retained-bundle locators", messages)

    def test_leaf_completion_rechecks_report_hash_and_actual(self) -> None:
        tampered_hash = "\n".join(str(issue) for issue in self.validate(record(sha256="0" * 64), require_complete=True))
        self.assertIn("local result SHA-256 does not match", tampered_hash)
        false_pass = "\n".join(str(issue) for issue in self.validate(record(actual="failed"), require_complete=True))
        self.assertIn("actual outcome is passed", false_pass)

    def test_rejects_missing_required_field_and_duplicate_event_identity(self) -> None:
        missing = record()
        del missing["actual"]
        messages = self.messages(missing, record())
        self.assertIn("missing required field: actual", messages)
        duplicate = self.messages(record(), record("1.1.2", event_id="event-1.1.1"))
        self.assertIn("duplicate evidence event_id event-1.1.1", duplicate)

    def test_repeated_task_id_requires_append_only_lineage_links(self) -> None:
        first = record()
        unlinked_retry = record(event_id="event-1.1.1-retry", subcheck_key="command-1.1.1-retry")
        messages = self.messages(first, unlinked_retry)
        self.assertIn("previous_index_sha256 must reference", messages)
        self.assertIn("previous_event_sha256 must reference prior event", messages)

    def test_allows_hash_linked_historical_events_for_one_task(self) -> None:
        first = record(status="stale", subcheck_key="command-historical")
        second = record(status="passed", subcheck_key="command-current", event_id="event-1.1.1-retry", previous_event_sha256=MODULE.canonical_event_sha256(first), previous_index_sha256=MODULE.canonical_event_sha256(first))
        self.assertEqual(self.validate(first, second), [])

    def test_rejects_duplicate_subcheck_and_preserves_one_l3_to_one_subcheck(self) -> None:
        messages = self.messages(record(), record("1.1.2", subcheck_key="command-1.1.1"))
        self.assertIn("closes more than one L3", messages)

    def test_terminal_subcheck_reservation_survives_later_stale_history(self) -> None:
        events = link_events(
            record("1.1.1", subcheck_key="shared-terminal-subcheck"),
            record("1.1.1", status="stale", subcheck_key="stale-transition"),
            record("1.1.2", subcheck_key="shared-terminal-subcheck"),
        )
        self.assertIn("closes more than one L3", "\n".join(str(issue) for issue in self.validate(*events)))

    def test_nonclosing_latest_states_are_valid_history_but_fail_completion(self) -> None:
        for status in ("failed", "blocked", "stale", "unexecuted"):
            item = record(status=status)
            self.assertEqual(self.validate(item), [])
            messages = "\n".join(str(issue) for issue in self.validate(item, require_complete=True))
            self.assertIn(f"latest status {status} cannot resolve", messages)
        mock_only = record(evidence_scope="trait-mock-only")
        self.assertEqual(self.validate(mock_only), [])
        messages = "\n".join(str(issue) for issue in self.validate(mock_only, require_complete=True))
        self.assertIn("latest trait/mock-only evidence cannot resolve", messages)

    def test_rejects_mutable_target_locator_and_inadequate_retention_policy(self) -> None:
        bad = retained_record(immutable_locator="release://local/target/evidence#sha256=" + "a" * 64, retention_policy="temporary")
        messages = self.messages(bad)
        self.assertIn("must not point into a mutable target/ path", messages)
        self.assertIn("retained-bundle retention_policy must be signed-release-evidence-bundle", messages)

    def test_rejects_mismatched_locator_hash(self) -> None:
        messages = self.messages(retained_record(immutable_locator="release://local/evidence#sha256=" + "b" * 64))
        self.assertIn("SHA-256 fragment must equal sha256", messages)

    def test_rechecks_retrieved_artifact_hash(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifact = Path(temporary) / "artifact.txt"
            manifest = Path(temporary) / "manifest.json"
            artifact.write_text("evidence", encoding="utf-8")
            digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
            locator = f"release://local/evidence#sha256={digest}"
            manifest.write_text(json.dumps({"schema_version": 1, "subcheck_key": "command-1.1.1", "artifact_sha256": digest, "artifact_locator": locator}), encoding="utf-8")
            manifest_digest = hashlib.sha256(manifest.read_bytes()).hexdigest()
            manifest_locator = f"release://local/manifest#sha256={manifest_digest}"
            item = retained_record(sha256=digest, immutable_locator=locator, artifact_manifest_sha256=manifest_digest, artifact_manifest_locator=manifest_locator)
            sources = {item["immutable_locator"]: artifact.as_uri(), item["artifact_manifest_locator"]: manifest.as_uri()}
            self.assertEqual(self.validate(item, verify=True, locator_map=sources), [])
            artifact.write_text("tampered", encoding="utf-8")
            issues = self.validate(item, verify=True, locator_map=sources)
            self.assertIn("SHA-256 mismatch", "\n".join(str(issue) for issue in issues))

    def test_verified_manifest_binds_subcheck_key_to_manifest_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifact = Path(temporary) / "artifact.txt"
            manifest = Path(temporary) / "manifest.json"
            artifact.write_text("evidence", encoding="utf-8")
            digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
            locator = f"release://local/evidence#sha256={digest}"
            manifest.write_text(json.dumps({"schema_version": 1, "subcheck_key": "wrong-subcheck", "artifact_sha256": digest, "artifact_locator": locator}), encoding="utf-8")
            manifest_digest = hashlib.sha256(manifest.read_bytes()).hexdigest()
            manifest_locator = f"release://local/manifest#sha256={manifest_digest}"
            item = retained_record(sha256=digest, immutable_locator=locator, artifact_manifest_sha256=manifest_digest, artifact_manifest_locator=manifest_locator)
            issues = self.validate(item, verify=True, locator_map={locator: artifact.as_uri(), manifest_locator: manifest.as_uri()})
            self.assertIn("manifest subcheck_key does not bind", "\n".join(str(issue) for issue in issues))

    def test_procedure_kind_requires_command_exit_or_manual_reviewer(self) -> None:
        command_messages = self.messages(record(exit_code_or_reviewer="not-an-exit-code"))
        self.assertIn("command procedure requires integer", command_messages)
        manual_messages = self.messages(record(procedure_kind="manual", exit_code_or_reviewer=0, artifact_or_command="review manual procedure/report v1"))
        self.assertIn("manual procedure requires reviewer identity", manual_messages)
        self.assertEqual(self.validate(record(procedure_kind="manual", exit_code_or_reviewer="architecture-reviewer", artifact_or_command="review manual procedure/report v1")), [])

    def test_not_applicable_requires_authoritative_policy_even_for_conditional_leaf(self) -> None:
        conditional = record(
            "16.2.2",
            status="not-applicable",
            procedure_kind="manual",
            exit_code_or_reviewer="release-integrator",
            not_applicable_condition="remote was fast-forward",
            adjustment_id="NA-16.2.2",
        )
        self.assertEqual(self.validate(conditional), [])
        messages = "\n".join(str(issue) for issue in self.validate(conditional, require_complete=True))
        self.assertIn("requires matching authoritative policy approval", messages)
        policy_task = {"task_id": "16.2.2", "priority": "P0", "release_blocking": True, "mandatory": False, "depends_on": [], "gate_ids": ["gate-16.2.2"], "not_applicable": {"approved": True, "approval_id": "NA-16.2.2"}}
        self.assertEqual(MODULE.completion_issues([(1, conditional)], {"16.2.2": policy_task}), [])

    def test_policy_cannot_invent_an_arbitrary_not_applicable_leaf(self) -> None:
        item = record("2.1.1", mandatory=False, status="not-applicable", procedure_kind="manual", exit_code_or_reviewer="reviewer", not_applicable_condition="fabricated", adjustment_id="NA-2.1.1")
        policy = {
            "schema_version": 1,
            "required_prefix_event_sha256": MODULE.canonical_event_sha256(item),
            "expected_ledger_sha256": MODULE.canonical_event_sha256(item),
            "tasks": [{"task_id": "2.1.1", "priority": "P0", "release_blocking": True, "mandatory": False, "depends_on": [], "gate_ids": ["gate-2.1.1"], "not_applicable": {"approved": True, "approval_id": "NA-2.1.1"}}],
        }
        messages = "\n".join(str(issue) for issue in self.validate(item, policy=policy, tasks="- [ ] 2.1.1 known leaf\n"))
        self.assertIn("may preapprove not-applicable only", messages)

    def test_mandatory_p1_cannot_be_not_applicable(self) -> None:
        item = record(
            "13.1.1",
            priority="P1",
            status="not-applicable",
            procedure_kind="manual",
            exit_code_or_reviewer="architecture-reviewer",
            not_applicable_condition="claimant says optional",
            adjustment_id="NA-13.1.1",
        )
        authority = {
            "13.1.1": {
                "task_id": "13.1.1",
                "priority": "P1",
                "release_blocking": True,
                "mandatory": True,
                "depends_on": [],
                "gate_ids": ["gate-13.1.1"],
                "not_applicable": {"approved": True, "approval_id": "NA-13.1.1"},
            }
        }
        messages = "\n".join(str(issue) for issue in MODULE.completion_issues([(1, item)], authority))
        self.assertIn("mandatory P0/P1 leaves cannot be not-applicable", messages)

    def test_claimant_self_asserted_bc_approval_is_not_authority(self) -> None:
        item = record("2.1.1", mandatory=False, status="not-applicable", exit_code_or_reviewer="reviewer", not_applicable_condition="scope correction")
        item["not_applicable_approval"] = {"adjustment_class": "B", "reviewer": "architecture-reviewer", "decision": "approved"}
        self.assertIn("unknown field: not_applicable_approval", "\n".join(str(issue) for issue in self.validate(item)))

    def test_authoritative_supersession_transitively_stales_and_allows_bound_revalidation(self) -> None:
        policy_tasks = {
            "1.1.1": {"task_id": "1.1.1", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": [], "gate_ids": ["gate-1.1.1"]},
            "1.1.2": {"task_id": "1.1.2", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": [], "gate_ids": ["gate-1.1.2"]},
            "1.1.3": {"task_id": "1.1.3", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": ["1.1.1"], "gate_ids": ["gate-1.1.3"]},
            "1.1.4": {"task_id": "1.1.4", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": ["1.1.3"], "gate_ids": ["gate-1.1.4"]},
        }
        old, replacement = link_events(record("1.1.1", status="superseded", replacement_task_id="1.1.2"), record("1.1.2"))
        old_hash = MODULE.canonical_event_sha256(old)
        dependent_stale, transitive_stale = link_events(
            old,
            replacement,
            record("1.1.3", status="stale", stale_for_supersession_event_sha256=old_hash),
            record("1.1.4", status="stale", stale_for_supersession_event_sha256=old_hash),
        )[2:]
        dependent_stale_hash = MODULE.canonical_event_sha256(dependent_stale)
        transitive_stale_hash = MODULE.canonical_event_sha256(transitive_stale)
        events = link_events(
            old,
            replacement,
            dependent_stale,
            transitive_stale,
            record("1.1.3", revalidation_of_event_sha256=dependent_stale_hash, revalidated_against_task_id="1.1.2"),
            record("1.1.4", revalidation_of_event_sha256=transitive_stale_hash, revalidated_against_task_id="1.1.2"),
        )
        self.assertEqual(MODULE._supersession_issues(list(enumerate(events, start=1)), policy_tasks), [])

    def test_authoritative_supersession_rejects_cycles_nonterminal_replacements_and_unbound_revalidation(self) -> None:
        policy_tasks = {
            task_id: {"task_id": task_id, "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": dependencies, "gate_ids": [f"gate-{task_id}"]}
            for task_id, dependencies in {"1.1.1": [], "1.1.2": [], "1.1.3": ["1.1.1"]}.items()
        }
        cycle = link_events(record("1.1.1", status="superseded", replacement_task_id="1.1.2"), record("1.1.2", status="superseded", replacement_task_id="1.1.1"))
        self.assertIn("replacement cycle", "\n".join(str(issue) for issue in MODULE._supersession_issues(list(enumerate(cycle, start=1)), policy_tasks)))
        nonterminal = link_events(record("1.1.1", status="superseded", replacement_task_id="1.1.2"), record("1.1.2", status="blocked"))
        self.assertIn("ends with latest status blocked", "\n".join(str(issue) for issue in MODULE._supersession_issues(list(enumerate(nonterminal, start=1)), policy_tasks)))
        source, replacement = link_events(record("1.1.1", status="superseded", replacement_task_id="1.1.2"), record("1.1.2"))
        source_hash = MODULE.canonical_event_sha256(source)
        stale = link_events(source, replacement, record("1.1.3", status="stale", stale_for_supersession_event_sha256=source_hash))[2]
        unbound = link_events(source, replacement, stale, record("1.1.3"))
        self.assertIn("must bind its post-supersession stale event", "\n".join(str(issue) for issue in MODULE._supersession_issues(list(enumerate(unbound, start=1)), policy_tasks)))

    def test_supersession_cannot_be_erased_or_revalidated_against_source(self) -> None:
        policy_tasks = {
            "1.1.1": {"task_id": "1.1.1", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": [], "gate_ids": ["gate-1.1.1"]},
            "1.1.2": {"task_id": "1.1.2", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": [], "gate_ids": ["gate-1.1.2"]},
            "1.1.3": {"task_id": "1.1.3", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": ["1.1.1"], "gate_ids": ["gate-1.1.3"]},
        }
        erased = link_events(record("1.1.1", status="superseded", replacement_task_id="1.1.2"), record("1.1.2"), record("1.1.1", subcheck_key="revived-source"))
        self.assertIn("must remain superseded", "\n".join(str(issue) for issue in MODULE._supersession_issues(list(enumerate(erased, start=1)), policy_tasks)))

        source, replacement = link_events(record("1.1.1", status="superseded", replacement_task_id="1.1.2"), record("1.1.2"))
        source_hash = MODULE.canonical_event_sha256(source)
        stale = link_events(source, replacement, record("1.1.3", status="stale", stale_for_supersession_event_sha256=source_hash))[2]
        stale_hash = MODULE.canonical_event_sha256(stale)
        wrong = link_events(source, replacement, stale, record("1.1.3", revalidation_of_event_sha256=stale_hash, revalidated_against_task_id="1.1.1"))
        self.assertIn("successor replacement", "\n".join(str(issue) for issue in MODULE._supersession_issues(list(enumerate(wrong, start=1)), policy_tasks)))

    def test_superseded_without_replacement_is_rejected(self) -> None:
        self.assertIn("requires one distinct replacement_task_id", self.messages(record(status="superseded")))

    def test_optionally_rejects_task_and_replacement_ids_absent_from_tasks_plan(self) -> None:
        plan = "- [ ] 1.1.1 known leaf\n"
        issues = self.validate(record("1.1.2"), tasks=plan)
        self.assertIn("task_id 1.1.2 is not an L3", "\n".join(str(issue) for issue in issues))
        self.assertEqual(self.validate(record(), tasks=plan), [])

    def test_deferred_task_marker_is_an_authoritative_l3(self) -> None:
        self.assertEqual(self.validate(record(), tasks="- [deferred] 1.1.1 deferred leaf\n"), [])

    def test_lineage_mapping_targets_must_exist_in_authoritative_tasks_plan(self) -> None:
        plan = "- [ ] 1.1.1 known leaf\n"
        mapping = {"format": "legacy-lineage-map-v1", "entries": [{"new_l3_ids": ["9.9.9"]}]}
        messages = "\n".join(str(issue) for issue in self.validate(record(), tasks=plan, lineage_mapping=mapping))
        self.assertIn("targets unknown L3 ID '9.9.9'", messages)

    def test_global_event_chain_rejects_removed_or_reordered_prefix(self) -> None:
        first = record("1.1.1")
        second = record("1.1.2", previous_index_sha256=MODULE.canonical_event_sha256(first))
        self.assertEqual(self.validate(first, second), [])
        self.assertIn("previous_index_sha256 must reference", "\n".join(str(issue) for issue in self.validate(second)))

    def test_closure_accepts_direct_leaf_result_without_release_resolver(self) -> None:
        item = record()
        policy = {
            "schema_version": 1,
            "required_prefix_event_sha256": MODULE.canonical_event_sha256(item),
            "expected_ledger_sha256": MODULE.canonical_event_sha256(item),
            "tasks": [{"task_id": "1.1.1", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": [], "gate_ids": ["gate-1.1.1"]}],
        }
        self.assertEqual(self.validate(item, policy=policy, tasks="- [ ] 1.1.1 known leaf\n"), [])
        self.assertIn("closure rejects an empty evidence ledger", "\n".join(str(issue) for issue in MODULE._closure_issues([], policy, {"1.1.1"}, {"1.1.1"}, "leaf")))

    def test_release_closure_fails_closed_until_signed_bundle_verifier_exists(self) -> None:
        item = record()
        policy = {
            "schema_version": 1,
            "required_prefix_event_sha256": MODULE.canonical_event_sha256(item),
            "expected_ledger_sha256": MODULE.canonical_event_sha256(item),
            "tasks": [{"task_id": "1.1.1", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": [], "gate_ids": ["gate-1.1.1"]}],
        }
        messages = "\n".join(str(issue) for issue in self.validate(item, policy=policy, tasks="- [ ] 1.1.1 known leaf\n", closure_kind="release"))
        self.assertIn("release closure is unavailable until task 1.1.8", messages)

    def test_closure_rejects_unresolved_retained_bundle(self) -> None:
        item = retained_record()
        policy = {
            "schema_version": 1,
            "required_prefix_event_sha256": MODULE.canonical_event_sha256(item),
            "expected_ledger_sha256": MODULE.canonical_event_sha256(item),
            "tasks": [{"task_id": "1.1.1", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": [], "gate_ids": ["gate-1.1.1"]}],
        }
        messages = "\n".join(str(issue) for issue in self.validate(item, policy=policy, tasks="- [ ] 1.1.1 known leaf\n"))
        self.assertIn("test locator map is unavailable for a local release evidence locator", messages)

    def test_scoped_closure_requires_only_the_selected_authoritative_leaf(self) -> None:
        item = record("1.1.1")
        other_policy_task = {"task_id": "1.1.2", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": [], "gate_ids": ["gate-1.1.2"]}
        policy = {
            "schema_version": 1,
            "required_prefix_event_sha256": MODULE.canonical_event_sha256(item),
            "expected_ledger_sha256": MODULE.canonical_event_sha256(item),
            "tasks": [{"task_id": "1.1.1", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": [], "gate_ids": ["gate-1.1.1"]}, other_policy_task],
        }
        self.assertEqual(self.validate(item, policy=policy, tasks="- [ ] 1.1.1 first\n- [ ] 1.1.2 second\n", closure_tasks={"1.1.1"}), [])

    def test_scoped_closure_requires_transitive_dependencies_and_task6_final_gate(self) -> None:
        final = record("6.4.7", depends_on=["6.1.1"])
        policy = {
            "schema_version": 1,
            "required_prefix_event_sha256": MODULE.canonical_event_sha256(final),
            "expected_ledger_sha256": MODULE.canonical_event_sha256(final),
            "tasks": [
                {"task_id": "6.1.1", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": [], "gate_ids": ["gate-6.1.1"]},
                {"task_id": "6.4.7", "priority": "P0", "release_blocking": True, "mandatory": True, "depends_on": ["6.1.1"], "gate_ids": ["gate-6.4.7"]},
            ],
        }
        messages = "\n".join(str(issue) for issue in self.validate(final, policy=policy, tasks="- [ ] 6.1.1 prerequisite\n- [ ] 6.4.7 final\n", closure_tasks={"6.4.7"}))
        self.assertIn("no latest evidence event for authoritative task 6.1.1", messages)

    def test_uitest_execution_is_rejected_before_task6_final_gate(self) -> None:
        for task_id in ("2.1.1", "6.4.6"):
            messages = self.messages(record(task_id, artifact_or_command="cargo run -p explorer-uitest -- --case forbidden"))
            self.assertIn("UITEST execution is ineligible", messages)


if __name__ == "__main__":
    unittest.main()
