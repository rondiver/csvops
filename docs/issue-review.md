# Review of the original implementation issues

Prepared with the 0.1.1 changes. These are proposed GitHub updates, to apply after the code is published and verified on the default branch. No issue has been changed by this document.

| Issue | Evidence in this version | Proposed disposition |
| --- | --- | --- |
| [#1 Core extraction](https://github.com/rondiver/csvops/issues/1) | Delimiter selection, BOM handling, quoted fields, and streaming input exist. CLI regressions cover malformed widths, late encoding failures, and headerless input. Headers are explicit: first row by default, `--no-header` to keep it as data. | Close with the documented header contract. Automatic guessing of whether a row is a header is outside this release. |
| [#2 Cardinality](https://github.com/rondiver/csvops/issues/2) | Exact counts transition to HyperLogLog above 10,000 distinct values. Unit tests cover the transition, duplicates, and estimation. UUID/name/uniqueness heuristics exist. | Close as implemented; retain the README's approximation limits. |
| [#3 Type inference](https://github.com/rondiver/csvops/issues/3) | Integer, float, boolean, datetime, string, and mixed-type detection exist. Profile date detection remains pattern-based, and differs from the validated drift parser; RFC 2822 support from the original checklist is absent. | Retitle **Align date inference with supported drift formats**. Add fixtures for invalid dates and explicit supported/unsupported formats; choose any added formats based on real input. |
| [#4 Statistics](https://github.com/rondiver/csvops/issues/4) | Welford statistics, reservoir percentiles, and sampled outlier heuristics are implemented and tested. Non-finite values and overflow now receive explicit handling. | Close as implemented. The reported outlier count applies to the retained sample. |
| [#5 Warnings](https://github.com/rondiver/csvops/issues/5) | Missing, mixed-type, outlier, and ID warnings exist. All-missing columns are critical; the original separate >95% missing threshold is not implemented. | Retitle **Decide missing-data severity thresholds** and narrow the body to whether >95% missing needs separate treatment. The current behavior is documented. |
| [#6 Drift](https://github.com/rondiver/csvops/issues/6) | Day/week/month buckets compare counts, missing rates, and numeric means. The implementation uses >20% relative mean change, not the original two-standard-deviation proposal. Categorical dominance change is absent. | Retitle **Evaluate additional drift signals on real exports**. Retain categorical change and alternative mean thresholds as possible follow-up work, with fixtures and a demonstrated use case. |
| [#7 Output](https://github.com/rondiver/csvops/issues/7) | Terminal and JSON reports exist. Independent CLI invocations now test repeatability beyond the sampling and frequent-value capacities; Unicode and no-color behavior have regression coverage. | Close as implemented. |
| [#8 Tests](https://github.com/rondiver/csvops/issues/8) | Existing unit tests plus end-to-end CLI regressions, with CI configured for three operating systems. The original property-testing and benchmark goals have not been completed. | Retitle **Establish representative CSV performance baselines**. Measure profiles with long fields, many unique values, and many time buckets before setting a speed or memory target. |

Suggested close comment for completed items:

> Implemented in the 0.1.1 release preparation. The README now describes the supported behavior and limitations, and the unit/CLI checks cover the relevant functionality. See CHANGELOG.md for the fixes and examples/ for reproducible input and output.

Use this comment only after linking the actual merged commit and passing checks. Do not describe proposed categorical drift, automatic header guessing, or an unmeasured performance target as delivered.
