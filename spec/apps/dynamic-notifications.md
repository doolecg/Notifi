# Dynamic notifications

Written for Notifi from how phones show live delivery and ride progress. No third-party code or text is used.

## What it does

- Reads the notifications Windows shows (toasts), including the ones websites send through Edge, Chrome and Firefox.
- When a notification comes from a delivery or ride service the user follows, the Island shows a live card: the service name, the stage ("Preparing", "On the way", "Arriving", "Delivered"), a progress fill and a countdown to the arrival time.
- Other notifications can optionally flash on the resting pill, and are listed in the Notifications module.

## Rules

- Everything is off until the user turns it on. Nothing is read, stored or sent anywhere. No sign-in, no passwords, no account access.
- Windows asks for notification access the first time. If it is refused the Island says so once and the feature stays idle.
- Followed services are a list in the settings. Defaults: Just Eat, Uber Eats, Uber, Deliveroo. A service matches when its name appears as a whole word in the app name or in the notification text, so browser notifications match too.
- A notification from a followed service that names no stage (offers, receipts, marketing) is not a progress card.
- Stages are found from words in the text, checking the furthest stage first. "Arriving" with more than 3 minutes left counts as "On the way".
- A number before "min" is the time left. The card counts down from it. Progress is the stage's share, or the elapsed share of the time left when that is further along, never above 95% before the stage is "Delivered".
- A delivered card goes after 90 seconds. A card with no news for 3 hours goes. A passed arrival time stops counting 20 minutes late.
- Notifications already in the Action Center when the feature starts are used for cards only; they do not flash on the pill.

## Not built

- Following an order by signing in to the service.
- A browser extension.
- More than one card at a time on the pill (the newest wins).
- Linux and macOS notification reading.
