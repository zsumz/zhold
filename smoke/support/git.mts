import type { SmokeContext } from "smoque";

export async function initializeGitRepository(
  t: SmokeContext,
  repository: string,
): Promise<void> {
  await t.cmd("git", ["init", "--quiet", "--initial-branch=main"], {
    cwd: repository,
  });
  await t.cmd("git", ["config", "user.name", "zhold smoke"], {
    cwd: repository,
  });
  await t.cmd("git", ["config", "user.email", "smoke@zhold.invalid"], {
    cwd: repository,
  });
  await t.cmd("git", ["add", "."], { cwd: repository });
  await t.cmd(
    "git",
    ["-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "smoke fixture"],
    { cwd: repository },
  );
}

export async function addGitWorktree(
  t: SmokeContext,
  repository: string,
  worktree: string,
): Promise<void> {
  await t.cmd(
    "git",
    ["worktree", "add", "--quiet", "-b", "smoke-linked", worktree],
    { cwd: repository },
  );
}
