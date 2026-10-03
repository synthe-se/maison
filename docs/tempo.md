# Tempo

EDF's Tempo tariff: RTE colours every day blue, white or red (22 red days and 43 white ones
per season, from 1 September to 31 August). Maison shows today's and tomorrow's colours, the
days left, the prices, and a forecast to J+7 that is labelled as one.

Code: `backend/src/tempo/` (`rules.rs` the rules, `forecast.rs` the model, `inputs.rs`
weather and consumption, `source.rs` the colours, `tariffs.rs` the prices, `mod.rs` the
service), `backend/src/bin/fit_tempo.rs` (the fitter, on the Mac only).

## Sources

| What | Where | Key |
|---|---|---|
| Colours (today, tomorrow from about 10:40, the seasons) | RTE's API `digital.iservices.rte-france.com/open_api/tempo_like_supply_contract/v1/tempo_like_calendars` when `RTE_CLIENT_ID` / `RTE_CLIENT_SECRET` are set, else RTE's open data `services-rte.com/cms/open_data/v1/tempo?season=` and `/tempoLight`, else `api-couleur-tempo.fr` | OAuth2 client credentials (free account on data.rte-france.com), else none |
| Quotas and days used | EDF `api-commerce.edf.fr/commerce-activet/api/v1/saisons/search?option=TEMPO` (checked against the colours counted; the CRE's 22/43 when EDF does not answer) | none |
| Prices (6 kVA, tax included) | data.gouv's CRE dataset (`DATE_DEBUT` is written year-day-month) | none |
| Consumption, wind, solar | ODRE éCO2mix `eco2mix-national-tr` (quarter-hours, as Tempo days 06:00 → 06:00) | none |
| Weather | Open-Meteo, ECMWF IFS 0.25°: 12 cities (temperature, weighted by population) and 12 wind-farm areas (wind at 100 m), one request each | none |

Nobody publishes an official forecast beyond tomorrow (RTE, EDF): J+2 to J+7 are Maison's.

## The forecast

RTE publishes how it picks the colours (« Méthode de choix des jours Tempo », indice 2 du
7/01/2025):

- the **net consumption** (consumption − wind − solar) of the Tempo day, **normalised** over the
  past year: `(C − q40) / ((q80 − q40) · e^(−γ(qT30 − k)))`, γ = −0.1176, k = 8.3042 °C, the
  quantiles of the past year's net consumption and temperatures;
- red when it crosses `3.15 − 0.010·day − 0.031·reds left` (1 November–31 March, Monday to
  Friday, no public holiday, at most 5 in a row), else white when it crosses
  `4.00 − 0.015·day − 0.026·(whites + reds left)` (never on Sunday), else blue;
- at the end of a period, what is left is placed whatever the weather (13 reds from 13 to 31
  March 2026).

Fed with the measured net consumption, this rule gives RTE's colour on 96 % of the days of
2014–2026. Maison estimates the net consumption from the weather forecast (three linear
models, as ratios of their past year) and runs the rule 400 times with an error that grows
with the horizon (correlated from day to day): the share of each colour is its probability.
A forecast costs about 70 µs on the Mac, a few milliseconds on the Pi 1.

### Measured (`model.json`, backtest on 2024-2025 and 2025-2026)

Each day of both seasons, a model fitted before the season forecast J+1 to J+7 from the
ECMWF forecasts issued that day, through the service's own code:

| | J+1 | J+2 | J+3 | J+4 | J+5 | J+6 | J+7 |
|---|---|---|---|---|---|---|---|
| Right, every day | 95.5 % | 95.5 % | 94.7 % | 93.3 % | 92.0 % | 89.2 % | 89.9 % |
| Right, November to March | 91.4 % | 91.4 % | 89.4 % | 86.1 % | 83.1 % | 77.2 % | 78.5 % |
| Red F1 | 0.93 | 0.95 | 0.91 | 0.87 | 0.83 | 0.75 | 0.73 |
| White F1 | 0.81 | 0.81 | 0.77 | 0.71 | 0.68 | 0.58 | 0.58 |

« Always blue » scores 81.8 % (every day); the former model scored 79 % (53 % in winter).
J+1 counts only before RTE publishes (after, tomorrow is RTE's).

## When a source is down

- Colours: the next source, then the season's file (`cached: true`).
- Weather: the last forecast (`weather.json`); its errors count from the day it was made
  (`stale: true`), and it stops where it stopped.
- ODRE: the normalisation keeps the past year it has (`netload.json`, 400 days).
- EDF: the CRE's quotas. data.gouv: the last prices.

## Refit, once a year (or after a change of the rules)

On the Mac, never on the Pi:

```bash
cargo run --release --manifest-path backend/Cargo.toml --bin fit_tempo   # --fresh: download again
```

It downloads RTE's seasons since 2014, éCO2mix since 2013, Open-Meteo's archive and its
past ECMWF forecasts (`backend/target/tempo-fit/`, kept), fits, replays the last full
seasons, prints the scores and writes `cache/tempo/model.json`, `netload.json` and the
seasons' files. Commit them, then `make deploy`: `deploy.sh` pushes `model.json` over the
Pi's (the rest of `cache/` only when newer).
