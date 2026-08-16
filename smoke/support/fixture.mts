import { join } from "node:path";

import type { PathRef, SmokeContext } from "smoque";

import { addGitWorktree, initializeGitRepository } from "./git.mts";
import { writeCargoProject } from "./project.mts";

export {
  addFailingBuildScript,
  addInterruptBuildScript,
  addWaitingBuildScript,
} from "./project.mts";

export interface WorktreeFixture {
  repository: string;
  store: string;
  worktree: string;
}

export interface RepositoryFixture {
  repository: string;
  store: string;
}

export async function createRepositoryFixture(
  t: SmokeContext,
  work: PathRef,
  name = "repository",
): Promise<RepositoryFixture> {
  const repository = work.path(name);
  const store = work.path("zhold-store");

  await writeCargoProject(t, repository);
  await initializeGitRepository(t, repository);
  return { repository, store };
}

export async function createWorktreeFixture(
  t: SmokeContext,
  work: PathRef,
): Promise<WorktreeFixture> {
  const fixture = await createRepositoryFixture(t, work);
  const worktree = work.path("linked-worktree");
  await addGitWorktree(t, fixture.repository, worktree);
  return { ...fixture, worktree };
}

export function finalArtifact(
  worktree: string,
  name = "zhold-smoke-fixture",
): string {
  const binary = process.platform === "win32" ? `${name}.exe` : name;
  return join(worktree, "target", "debug", binary);
}
