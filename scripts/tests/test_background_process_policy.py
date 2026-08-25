import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "check_background_process_policy.py"
SPEC = importlib.util.spec_from_file_location("background_policy", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class BackgroundProcessPolicyTests(unittest.TestCase):
    def _fixture(self, source: str, sites: list[dict]) -> tuple[tempfile.TemporaryDirectory, Path, Path]:
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        source_path = root / "crates" / "sample" / "src" / "lib.rs"
        source_path.parent.mkdir(parents=True)
        source_path.write_text(source, encoding="utf-8")
        inventory = root / "inventory.json"
        inventory.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "file_classifications": [],
                    "sites": sites,
                }
            ),
            encoding="utf-8",
        )
        return temporary, root, inventory

    def test_unclassified_production_launch_reports_source_line(self):
        temporary, root, inventory = self._fixture(
            "fn run() { let _ = Command::new(\"tool.exe\"); }\n", []
        )
        self.addCleanup(temporary.cleanup)
        errors = MODULE.validate(root, inventory)
        self.assertEqual(
            errors,
            ["crates/sample/src/lib.rs:1: unclassified production process launch"],
        )

    def test_test_module_launch_does_not_count_as_production(self):
        source = """
fn run() {
    let mut command = Command::new("tool.exe");
    configure_background_command(&mut command);
}
#[cfg(test)]
mod tests {
    fn fixture() { let _ = Command::new("fixture.exe"); }
}
"""
        site = {
            "path": "crates/sample/src/lib.rs",
            "anchor": "let mut command = Command::new(\"tool.exe\");",
            "classification": "hidden-background",
            "required_anchor": "configure_background_command(&mut command);",
            "rationale": "fixture",
        }
        temporary, root, inventory = self._fixture(source, [site])
        self.addCleanup(temporary.cleanup)
        self.assertEqual(MODULE.validate(root, inventory), [])


if __name__ == "__main__":
    unittest.main()
