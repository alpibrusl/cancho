# One pharma supply chain, three analytics and ML use cases, one digital twin (`examples/usecases/`)

**Status: design, written before any code (version 2: the three systems are now one connected world).** Nothing here is a result.
Every claim is **[measured]** (with where), **[read]** (with the file or command it was read from), a **[choice]**, or **[open]**. A
claim found false is corrected here, in place. Nothing here is medical or regulatory advice, and no real patient data is used or needed.

## 1. What this is for

The tools (`table`, `stats`, `ml`, `solve`, `opt`, `flow`, and later `plot`) are each checked alone. No example uses them *together* on a
question whose answer someone would act on. This document designs one: a **pharmaceutical distribution chain**, seen from three places,
with a **digital twin** that generates realistic fake data for all three from one stated model, so the example runs anywhere, in CI,
with a known truth to check every analysis against.

```
 patients ──refill demand──▶ PHARMACIES ──orders──▶ DEPOT ──EV reefer trucks──▶ PHARMACIES
     ▲                           │  stockouts, expiry         │  late or warm deliveries
     └──────── lapse (stops refilling) ◀──────────────────────┴───────────────────────┘
```

| | use case | question the example answers | tools |
|---|---|---|---|
| **C** | pharmacy replenishment | how many units of each medicine will each pharmacy dispense next week, and what do we order given expiry dates, cold-chain items and a shelf-space cap | table, stats, ml, solve, opt, flow |
| **A** | cold-chain delivery with electric trucks | which truck serves which pharmacies, will its battery last given the refrigeration load, and how do we charge the fleet overnight under the depot's power cap | table, stats, ml, solve, opt, flow |
| **B** | refill adherence ("churn") | which chronic patients will stop refilling, is the model's probability honest, and which patients get a pharmacist call or a home delivery for a fixed budget | table, stats, ml, solve, flow |

**The three are coupled, and that is the point [choice].** A stockout at a pharmacy (C) raises a patient's chance of lapsing (B); a late
or warm delivery (A) causes the stockout and the discarded stock; the patients' refills are the demand that C forecasts and A
delivers. Each use case is non-trivial alone (time and group structure a random split would get wrong, a prediction that feeds a
decision, a decision that is an optimisation with a checkable answer); together, a number in one is an input to another, and the twin
knows by how much.

## 2. The twin

**A twin is a generative simulator of the whole chain, not a live mirror of a real one [choice].** It is a function
`(world, seed, size) -> files`. The `world` is a JSON file of named parameters; the twin writes the CSV files each use case reads, and
`truth.json`.

Properties, each a gate (section 7) before it is code:

* **T1 deterministic [choice].** The same world, seed and size give the same bytes, on every platform. The twin brings its own generator
  (a 64-bit splitmix, and `math` for the rest), not `random`, whose stream is a property of the Python version.
* **T2 the truth is a file [choice].** `truth.json` holds the parameters that generated the data and the quantities an analysis should
  recover (an effect, a noise floor, the best plan's cost). The analysis never reads it; the gates do.
* **T3 realistic where it matters [choice].** Each parameter in `world.json` says whether it is a **stylised fact (choice)**, **read** from
  a named public source, or **fitted** to a file. Today none is fitted (section 3).
* **T4 coupled, with the coupling switchable [choice].** Every cross-use-case effect (stockout to lapse, late delivery to stockout, forecast
  error to expiry write-off) has a coefficient that can be set to zero, so a gate can show the effect is found when present and not
  found when absent.
* **T5 sized by a flag [choice].** `--size small` runs in seconds for CI; `--size large` is for a measured run on one machine.
* **T6 stdlib only [choice].** Python 3 and nothing to install, like the repositories' scripts.
* **T7 synthetic people only [choice].** Patients are rows of generated numbers. The twin has no input that is a real person's data.

## 3. Realistic data: what is reachable

*Measured in the session that wrote this (2026-11-11, through the agent proxy):* the IBM sample file `Telco-Customer-Churn.csv` on
`raw.githubusercontent.com` answered HTTP 200; `archive.ics.uci.edu` and `ev.caltech.edu` were refused (403). No pharmacy, drug-sales,
adherence or cold-chain dataset was looked for yet. So:

* **All three twins start from stylised facts, marked as choices.** Examples of what they will be: refill intervals of 30 or 90 days with
  a spread; a share of items that need 2 to 8 degrees Celsius **[choice]** (the usual refrigerated range; **[open]** check against the
  distribution guidance a real operator follows before the README says anything about compliance); shelf lives of months to years; winter
  peaks for respiratory medicines.
* **[open]** a public dataset for each of: pharmacy sales by item and day, patient refill histories (claims-style), vehicle energy per
  kilometre with a refrigeration unit. Each is a file the maintainer can place in a directory; the calibration step (section 5, `opt`)
  runs on any file with the twin's columns, so a real file moves a parameter from *choice* to *fitted* without changing the code.
* **The Telco file** is a churn sample from another industry. It is **not** used to calibrate the pharma twin. **[open]** whether to use it
  once, to run the churn half of the pipeline on a real file as a smoke test; its licence is unread, so nothing derived from it is
  published until it is.

## 4. The three use cases

### 4.1 C: pharmacy replenishment under expiry and cold chain

*What makes it pharma.*
* **Expiry and FEFO.** Stock is in lots with an expiry date; the pharmacy dispenses first-expiry-first-out; stock that expires is a
  write-off. Over-ordering costs money that a retail example for biscuits would not lose so fast.
* **Cold-chain items** have a smaller shelf-space cap and a temperature constraint on delivery (A).
* **Two kinds of demand.** Chronic refills are predictable from patients' due dates (B); acute demand (a season's infections) is not.
* **Asymmetric stockout cost.** A missing critical medicine costs more than a missing vitamin; the cost enters the objective and a
  service-level target.
* **Substitution.** A stocked-out brand can be replaced by a generic; the twin has a substitution rate, so a stockout does not always
  lose the sale **[choice]**; the demand the data records is therefore censored in a more complicated way than in retail.
* **Controlled items** have a per-order cap **[choice]**.

*Twin files.* `pharmacies.csv`, `items.csv` (shelf life, cold chain, criticality, pack size), `dispensing.csv` (pharmacy, item, day,
units), `stock.csv` (pharmacy, item, day, lot, expiry, units), `orders.csv`.

*Pipeline.*
1. `table` builds the weekly panel, joins items and lots, and computes lags and days-to-expiry (SQL, one `flow step` each).
2. `stats`: `describe` and `corr` (demand against temperature and season), `ks` / `mannwhitney` (a respiratory item in winter against
   summer), the share of cells censored by stockout per item.
3. `ml cv --split-by time:week`, then `ml fit --model boost` (baselines: `constant`, last week). The refill share of demand from B is a
   feature.
4. `solve`: a replenishment MIP per pharmacy: order quantity per item (integer packs), shelf-space and cold-space caps, expected write-off
   from the lots' expiry dates, a service-level constraint from the forecast's held-out error, the controlled-item cap.
5. `opt minimize`: calibrates the substitution and elasticity parameters of the twin to a file.
6. `flow`: every step recorded.

### 4.2 A: cold-chain delivery with electric reefer trucks

*What makes it pharma.*
* **A temperature band on the whole trip.** The refrigeration unit draws power from the traction battery, more when it is hot outside and
  while doors are open at a stop; so a route's energy is *driving + refrigeration*, and a cold item raises it **[choice]**.
* **Time windows** at pharmacies, and a deadline (a pharmacy opens at 08:00 and must have the delivery before it).
* **A warm delivery is a loss** (the batch is discarded) and shows in C's write-off; a **late** one shows in C's stockouts.
* **The depot has a power cap** shared by overnight charging and the refrigerated warehouse.

*Twin files.* `trucks.csv` (battery kWh, refrigeration kW, cold capacity), `routes.csv` (day, truck, ordered stops, distance, time
windows, outside temperature), `telemetry.csv` (route, stop, kWh used, cabin temperature, door-open minutes), `depot.csv` (hourly cap and
tariff).

*Pipeline.*
1. `table` builds one row per route and per stop, joined to weather and to the orders that C produced.
2. `stats`: the refrigeration effect on energy (`corr`, a `t2` of hot against mild days), door-open time against temperature excursions
   (`test chisq` or `prop` with `--correct holm`).
3. `ml cv --split-by time:day`, `ml fit --model boost|linear`: (i) a regression for a route's energy, with a baseline that is distance
   alone; (ii) a classification for the risk of a temperature excursion, with `ml calibrate`.
4. `solve`: (i) assign the day's deliveries to trucks so that each truck's predicted energy plus a reserve fits its battery and every time
   window holds, and (ii) the overnight charging plan under the depot cap at the tariff, so that each truck is full by its departure.
   Stop order within a truck is part of the model only up to a size `solve` is shown to handle (section 9, open 5); beyond it the twin
   gives the order as data and the example says so.
5. `opt minimize`: calibrates the twin's refrigeration and driving coefficients to a telemetry file.
6. `flow`.

### 4.3 B: refill adherence

*What makes it pharma.* "Churn" is **lapse**: a chronic patient who does not refill within a grace period after the due date. It matters
clinically and commercially, and the interventions are different from a telecom's discount: a pharmacist's call, a reminder, a switch
to a 90-day supply, a home delivery (which loads A).

*Twin.* Patients with an age band, a regimen (number of chronic items, refill interval), a co-payment, a home pharmacy, and a refill history
generated by a hazard model: a base lapse rate, an effect of regimen complexity and co-payment, an effect of **a stockout of the
patient's item at the pharmacy in the last 30 days** (from C) and of **a late delivery** (from A). Files: `patients.csv`, `refills.csv`
(patient, item, due, filled), `contacts.csv` (outreach already made).

*Pipeline.*
1. `table` builds one row per patient and due date: days since the last fill, gaps so far, stockouts seen, contacts made.
2. `stats`: `test prop` and `chisq` (lapse by regimen size, by co-payment band, by a stockout seen; `--correct holm`), `mannwhitney` (gap
   lengths of lapsers against others), each with an interval.
3. `ml cv --split-by time:due` (a patient's later due dates must not train a model that scores an earlier one), `ml fit --model
   logistic|forest|boost`, `ml eval`, `ml importance`; `ml calibrate` or `ml isotonic`, and a check that a predicted 30 percent is a lapse
   rate of about 30 percent in the held-out rows.
4. `solve`: choose patients and an intervention each (call, reminder, 90-day switch, home delivery) maximising expected adherent days (the
   calibrated probability times an uplift the twin knows) under the pharmacists' hours, a budget and the delivery slots A has left.
5. `flow`.

*Optional second kind of churn [open]:* an independent pharmacy leaving the wholesaler (a B2B account), from the same twin's
`pharmacies.csv` and its service history (fill rate, late deliveries). Same pipeline, a few hundred rows, a harder statistical problem
(small groups, exact tests: `fisher`). Left out unless the maintainer wants it.

## 5. Figures

`plot` is not in the pinned set of this repository's work yet and **its issues stay open for another session [decision of the
maintainer, 2026-11-12]**. Until then the figures are `--term dumb` text or tables printed by `table`. Nothing in the design depends on
`plot`; section 9 lists what it would add (calibration curve, forecast against actual, heat map of stockouts, load against the power
cap).

## 6. What `flow` does here, and what it does not

Each stage is one recorded `flow step` or a `flow run` plan, with its inputs and outputs declared and hashed, so `flow-records check`
can say that a number in the README is the output of these inputs [choice]. `flow` stays what cancho-flow's `CLAUDE.md` says it is: not a
scheduler, not a shell. If a stage needs more than a list of argument lists, it is a program called by `flow step`.

**[open]** how a Python twin is started: `flow` starts only programs in its baked directory (cancho-flow's `docs/design.md`), so either that directory holds
a `python3` entry or the twins are cancho programs. Milestone U1 settles it before anything depends on it.

## 7. Gates, written before the code they judge

Each must be able to fail (a mutant of the twin or the pipeline that it catches):

| gate | what it holds | bound |
|---|---|---|
| **G1** twin determinism | two runs give the same bytes; two seeds give different bytes | exact |
| **G2** truth recovered | each effect the twin put in is found by `stats` (right sign, an interval that holds the truth) and is **not** found when its coefficient is set to zero | the intervals and the false-positive rate, fixed in U2 before measuring |
| **G3** coupling is real | with a coupling coefficient set to zero the downstream effect vanishes (a stockout no longer moves lapse; a late delivery no longer moves stockouts); at its value it is found | as G2 |
| **G4** the model earns its place | on a time split each model beats its baseline by a margin fixed in advance and is not better than the twin's noise floor | margin fixed before the first run **[open]** |
| **G5** the decision is checked | `solve verify` accepts every plan; on the small size each plan's cost equals an independent enumeration or HiGHS (scipy, optional in CI) | exact on small, a gap on large |
| **G6** constraints are physical | no plan puts a truck on a route its predicted energy plus reserve exceeds the battery for; no plan orders more than the shelf or cold cap; a plan that does is refused by `solve verify` | exact |
| **G7** the record is a proof | `flow-records check` passes; changing one byte of an input makes it fail | exact |
| **G8** calibration (B and the excursion classifier) | held-out predicted against observed in each decile within a bound fixed in advance | **[open]** |
| **G9** no panics, every refusal tagged | a fuzz of the pipeline's arguments, as in each repository | the tools' own |

Mutants for the twin: a changed rate, a dropped effect, a changed seed mix, a coupling that is wired to the wrong sign, an off-by-one in
the days-to-expiry. All killed, or each survivor explained.

## 8. Milestones, each its own pull request

| | what | done when |
|---|---|---|
| **U0** | this document, reviewed | the maintainer says go; the **[open]** items answered or accepted |
| **U1** | the twin library (generator, world file, `truth.json`), G1, and how `flow` starts it (section 6) | G1 passes and fails on its mutants |
| **U2** | C and the coupling C to B (the lapse model sees stockouts), with G2, G3, G4, G7 | the README output is printed by `flow` records |
| **U3** | B end to end, with G8 | same |
| **U4** | A end to end, with G5, G6 and the `opt` calibration; the coupling A to C | same |
| **U5** | the closed loop: a week of the whole chain, one command | the loop's numbers are reproducible bytes |
| **U6** | a measured run on one machine (machine, threads, load, minimum of N) and the claims the numbers support | numbers in section 10 |

The order is C, B, A because C produces what B and A consume; **[open]** whether A (the most solver-heavy) should come earlier.

## 9. Open

1. **Where it lives. [decided by the maintainer, 2026-11-12]** In this repository: this document in `docs/`, the example in
   [`examples/usecases/pharma-chain/`](../examples/usecases/pharma-chain/) (index: [`examples/usecases/`](../examples/usecases/)). The
   example uses the built binaries of cancho-table, -stats, -ml, -solve, -opt and -flow at commits it pins, fetched and built by a script
   in that folder (U1), never copied here. `cargo test` walks `examples/` for `.cho` programs directly inside each top-level folder
   (`crates/cancho/tests/conformance/selfhost/authority.rs`, read 2026-11-12); a folder holding only markdown, scripts and subfolders
   is not scanned. **[open]** whether the twin, being Python, belongs in a repository whose examples are programs meant to be read
   in cancho; the alternative is a twin written in cancho (section 6).
2. A reachable public dataset for pharmacy sales, for refill histories and for refrigerated-vehicle energy (section 3).
3. The Telco file: whether to use it as a smoke test, and its licence (section 3).
4. How the Python twin is started by `flow` (section 6).
5. **How large a routing model `solve` handles**, measured in U4 before the design depends on it: a stop order within a truck is a small
   travelling-salesman problem; the example uses it only as far as the measurement allows.
6. The margins of G4 and G8, fixed before the first measurement and recorded here.
7. Whether `ml` has a forecasting form beyond features and lags with a time split; this design assumes it does not, and says so in the README.
8. **`plot`** (cancho-plot) is a seventh repository and pins an older compiler (`cancho = "9934b11"`, in its `cancho.toml`, read 2026-11-12).
   The session that wrote this has read access only. *Measured 2026-11-12:* it builds with cancho `51524d2` and writes a text chart from
   CSV on a Linux x86-64 machine; nothing more was run. Its pin move and its figures are for another session, by the maintainer's decision.
9. **Domain review.** The pharma specifics (cold-chain band, controlled-item caps, shelf lives, adherence grace period) are choices; a
   person who works in this field should review them before the README presents the example as realistic.

## 10. Results

None yet.
