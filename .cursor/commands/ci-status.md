# CI Status

Report the latest GitHub Actions result for the current branch.

1. Get the current branch with `git branch --show-current`.
2. Find the latest run with `gh run list --branch <branch> --limit 1`.
3. Delegate to the `reviewer` subagent with the branch name and run ID. Ask for its standard compact output.
4. Summarize for me:
   - Status (green, red, or in progress) with a link to the run
   - For failures: each root cause with file and line, separated from cascade errors
   - The suggested next action

Do not pull full logs into this conversation.
