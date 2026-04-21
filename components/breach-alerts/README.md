# breach-alerts

Stores and retrieves breach alert dismissals by breach name. HIBP identifies breaches by name
rather than by a numeric ID; the name is unique and serves as the stable identifier.

## Data Types

### `BreachAlertDismissal`

| Field | Type | Description |
|---|---|---|
| `breach_name` | `String` | The HIBP breach name. HIBP identifies breaches by name rather than a numeric ID; the name is unique and serves as the stable identifier. |
| `time_dismissed` | `i64` | Unix timestamp in milliseconds of when the breach alert was last dismissed. |

## API

- `get_breach_alert_dismissals(breach_names)` — returns matching dismissals
- `set_breach_alert_dismissals(dismissals)` — upserts dismissals
- `clear_breach_alert_dismissals(breach_names)` — deletes specific dismissals
- `clear_all_breach_alert_dismissals()` — deletes all dismissals
