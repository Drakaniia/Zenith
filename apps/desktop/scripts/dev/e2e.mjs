// End-to-end checks against an isolated Zenith test instance (debug build, own profile and
// remote-debugging port; see run.ps1). The suite lives in ./e2e/ (index.mjs runs it).
//   powershell -File scripts/dev/run.ps1 -Fresh -SkipSetup
//   node scripts/dev/e2e.mjs [--only name,name] [--skip-restart]
import "./e2e/index.mjs";
