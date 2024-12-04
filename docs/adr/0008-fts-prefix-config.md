# FTS prefix configuration

* Status: Accepted
* Deciders: Ben Dean-Kawamura, Will Stuckey, Drew Willcoxon, Nan Jiang
* Date: 2024-07-25


## Context and Problem Statement

The Suggest component will use the [SQLite FTS5 extension](https://sqlite.org/fts5.html) to index suggestions for the Fakespot project.
FTS queries will usually be prefix queries -- if the user types in "abcd" we will match any word that begins with "abcd".
The FTS extension supports configurable [prefix indexes](https://sqlite.org/fts5.html#prefix_indexes) to optimize these queries.
This ADR will explore different parameters for the prefix index configuration and decide on a value for the Fakespot experiment.

This decision was based on benchmarks from the current code and data set.
We should revisit this in the future if the Fakespot data set or query strategy changes or for new FTS-based suggestion providers.
Hopefully it can serve as a starting point for these future decisions.


## Considered Options

In general, prefix indexes can optimize up certain queries at the cost of database size and ingestion time.
Increased database size will often slow down the non-optimize queries.
This ADR will benchmark and compare a large range of options.

### A) no prefix index
### B) prefix='1'
Index 1 character prefix queries.

### C) prefix='1 2'
Index 1 and 2 character prefix queries.
The next set of options continue trend of adding an index to optimize the next number of chars.

### D) prefix='1 2 3'
### E) prefix='1 2 3'
### F) prefix='1 2 3 4'
### G) prefix='1 2 3 4 5'
### H) prefix='1 2 3 4 5 6'
### I) prefix='1 2 3 4 5 6 7'
### J) prefix='2 3 4'
Index 2, 3, 4 character prefix queries.
This option is intended to test if it's useful to index higher character count prefix queries, but not lower count ones.

## Decision Outcome

### A) No prefixes

## Comparison to Other Options

This option results in the least DB size and fastest ingestion time.  In some cases, it has a higher
query time -- especially for the 1-character prefix query benchmark.  However, the worst-case query
time was around 1ms which was deemed acceptible.

## Benchmarks

These were calculated using the preliminary fakespot data set of 10,000 items.
They ran on a new-ish Dell laptop.

### Database size

DB sizes are calculated using `page_size * num_pages`, which means some of the larger steps should
be taken with a grain of salt since they may be caused by a small change overflowing the page size.


| prefix setting            | DB size |
| ------------------------- | ------- |
|none                       | 2896kb  |
|'1'                        | 3088kb  |
|'1 2'                      | 3293kb  |
|'1 2 3'                    | 3797kb  |
|'1 2 3 4'                  | 3895kb  |
|'1 2 3 4 5'                | 3932kb  |
|'1 2 3 4 5 6'              | 4104kb  |
|'1 2 3 4 5 6 7'            | 4293kb  |
|'2 3 4'                    | 3801kb  |

### Ingestion time

Extra prefix indexes cause extra writes during ingestion, which slows it down.
These times were calculated using the `ingest-fakespot` and `ingest-again-fakespot` benchmarks.

| prefix setting            | Ingestion time | Reingesiton time |
| ------------------------- | -------------- | ---------------- |
|none                       | 202.71 ms      | 308.84 ms        |
|'1'                        | 226.27 ms      | 324.28 ms        |
|'1 2'                      | 256.54 ms      | 404.14 ms        |
|'1 2 3'                    | 298.22 ms      | 534.09 ms        |
|'1 2 3 4'                  | 332.14 ms      | 577.71 ms        |
|'1 2 3 4 5'                | 352.22 ms      | 537.47 ms        |
|'1 2 3 4 5 6'              | 367.08 ms      | 543.84 ms        |
|'1 2 3 4 5 6 7'            | 376.16 ms      | 605.72 ms        |
|'2 3 4'                    | 302.76 ms      | 524.85 ms        |

### Query time

Queries were chosen to perform prefix matches with various character lengths.

The Fakespot query code will only do a prefix match if the total input length is > 3 chars.
Therefore, to test shorter prefixes we use 2-term queries.

These benchmarks had some noise, but results generally fell within a few percentage points of each other when repeated.
The benchmarks represent repeated queries with a warm cache.
Slow disks and/or an empty cache may change the results.


|prefix|query|time|change|
|------|-----|----|------|
|none|hand s|1.1461 ms|+0.00%|
|'1'|hand s|273.41 µs|-76.12%|
|'1 2'|hand s|328.89 µs|-71.29%|
|'1 2 3'|hand s|324.70 µs|-71.70%|
|'1 2 3 4'|hand s|267.43 µs|-76.76%|
|'1 2 3 4 5'|hand s|306.71 µs|-73.19%|
|'1 2 3 4 5 6'|hand s|297.68 µs|-74.00%|
|'1 2 3 4 5 6 7'|hand s|306.49 µs|-73.16%|
|'2 3 4'|hand s|765.71 µs|-32.84%|
|none|hand sa| 90.456 µs|+0.00%|
|'1'|hand sa|121.57 µs|+34.28%|
|'1 2'|hand sa|90.285 µs|+0.04%|
|'1 2 3'|hand sa| 83.731 µs|-7.42%|
|'1 2 3 4'|hand sa|72.730 µs|-19.58%|
|'1 2 3 4 5'|hand sa|70.994 µs|-21.55%|
|'1 2 3 4 5 6'|hand sa|68.499 µs|-24.39%|
|'1 2 3 4 5 6 7'|hand sa|70.367 µs|-22.12%|
|'2 3 4'|hand sa|69.000 µs|-23.76%|
|none|hand san|48.971 µs|+0.00%|
|'1'|hand san|78.062 µs|+59.51%|
|'1 2'|hand san|81.759 µs|+66.87%|
|'1 2 3'|hand san| 64.598 µs|+32.02%|
|'1 2 3 4'|hand san|56.380 µs|+15.04%|
|'1 2 3 4 5'|hand san|52.830 µs|+7.94%|
|'1 2 3 4 5 6'|hand san|50.669 µs|+5.22%|
|'1 2 3 4 5 6 7'|hand san|53.176 µs|+8.33%|
|'2 3 4'|hand san|50.977 µs|+3.90%|
|none|sani|113.76 µs|0.00%|
|'1'|sani|146.09 µs|+27.91%|
|'1 2'|sani|162.32 µs|+42.07%|
|'1 2 3'|sani|155.34 µs|+35.89%|
|'1 2 3 4'|sani|129.23 µs|+13.93%|
|'1 2 3 4 5'|sani|142.33 µs|+24.02%|
|'1 2 3 4 5 6'|sani|134.78 µs|+18.47%|
|'1 2 3 4 5 6 7'|sani|137.85 µs|+20.56%|
|'2 3 4'|sani|129.39 µs|+13.84%|
|none|sanit|116.27 µs|+0.00%|
|'1'|sanit|148.44 µs|+27.64%|
|'1 2'|sanit|166.03 µs|+42.61%|
|'1 2 3'|sanit|157.65 µs|+35.42%|
|'1 2 3 4'|sanit|139.61 µs|+18.70%|
|'1 2 3 4 5'|sanit|148.12 µs|+27.13%|
|'1 2 3 4 5 6'|sanit|138.13 µs|+19.12%|
|'1 2 3 4 5 6 7'|sanit|143.87 µs|+23.66%|
|'2 3 4'|sanit|135.68 µs|+16.42%|
|none|saniti|23.987 µs|+0.00%|
|'1'|saniti|56.012 µs|+133.72%|
|'1 2'|saniti|54.549 µs|+126.89%|
|'1 2 3'|saniti|46.341 µs|+92.85%|
|'1 2 3 4'|saniti|43.708 µs|+83.68%|
|'1 2 3 4 5'|saniti|36.161 µs|+50.69%|
|'1 2 3 4 5 6'|saniti|30.129 µs|+25.35%|
|'1 2 3 4 5 6 7'|saniti|32.442 µs|+35.11%|
|'2 3 4'|saniti|25.379 µs|+5.51%|
