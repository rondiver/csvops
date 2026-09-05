# An invented job export

`jobs.csv` contains 12 synthetic records across two ISO weeks. The names, quantities, and amounts are invented; no customer or operating records are included.

- Week 35 has four jobs, with quantities present in all four.
- Week 36 has eight jobs; six quantities are missing, for a missing rate of 75%.
- Mean quoted total moves from 100 to 200.
- Across the whole file, six of twelve quantities are missing. The mean of the six known quantities is 1,000.

From the repository root:

```sh
csvops profile examples/jobs.csv --json
csvops drift examples/jobs.csv --time-col created_at --grain week --json
csvops drift examples/jobs.csv --time-col created_at --grain week --no-color
```

The complete generated outputs are [profile.json](profile.json), [drift.json](drift.json), and [drift.txt](drift.txt). The profile's file-size field reflects the checked-in file's bytes and can differ if a checkout converts line endings. The tests check the known data relationships independently of these saved reports.

This example demonstrates what changed in an export. Investigation would still be needed to establish its cause.
