-- SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
--
-- SPDX-License-Identifier: Apache-2.0

import CctpSpec.Fixtures

/-- Prints the correspondence fixture JSON to stdout, or fails loudly if any
vector disagrees with the Lean model. -/
def main : IO UInt32 := do
  match CctpSpec.Fixtures.fixturesJson with
  | .ok json =>
    IO.println json.pretty
    return 0
  | .error e =>
    IO.eprintln s!"fixture generation failed: {e}"
    return 1
