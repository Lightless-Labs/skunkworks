#!/usr/bin/env node
import { runHookCli } from "../src/core/hookCli.ts";

const hostArg = process.argv.find((arg) => arg.startsWith("--host="));
const host = hostArg?.slice("--host=".length) as any;
await runHookCli({ host: host || "generic" });
