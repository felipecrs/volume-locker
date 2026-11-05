# Localization

Volume Locker's interface text lives in [Fluent](https://projectfluent.org/) files under [`i18n/`](../i18n/), one folder per language. All languages are built into the executable, so it stays a single portable file.

The app follows the Windows display language at startup. Text that has no translation appears in English.

## Adding a language

1. Copy `i18n/en/volume_locker.ftl` to `i18n/<language tag>/volume_locker.ftl`, for example `i18n/pt-BR/volume_locker.ftl`.
2. Translate the text to the right of each `=`. Keep the `{ $name }` arguments, moving them wherever the sentence needs them.
3. Run `mise run test`. It reports messages that no longer exist in English and arguments that do not exist.

You can leave out messages you have not translated yet.

## Adding text to the app

Add the message to the English file, then use it with `t!("message-id", argument = value)`. The build fails if the ID or an argument name does not exist in the English file.

- Write whole sentences as one message, since word order differs between languages.
- Keep log messages in English so bug reports stay readable. `log_and_notify_error` takes the log message separately for this reason.
