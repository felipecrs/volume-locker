# Localization

Volume Locker follows the Windows display language at startup and shows English for any text without a translation.

Translations are [Fluent](https://projectfluent.org/) files in [`i18n/`](../i18n/), one folder per language. They are built into the executable, so it stays a single portable file.

## Add a language

1. Copy `i18n/en/volume_locker.ftl` to `i18n/<language tag>/volume_locker.ftl`, for example `i18n/pt-BR/volume_locker.ftl`.
2. Translate the text to the right of each `=`. You can leave out messages you have not translated yet.
3. Keep each `{ $name }` argument, and move it wherever the sentence needs it.
4. Run `mise run test`.

The test fails if the file is named wrong, a message no longer exists in English, or an argument does not exist.

## Add text to the app

1. Add the message to `i18n/en/volume_locker.ftl`.
2. Use it in the code with `t!("message-id", argument = value)`.

The build fails if the message ID or an argument name is not in the English file.

Write each sentence as one whole message, because word order differs between languages.

Logs stay in English so bug reports are readable:

- Use `t_en!` instead of `t!` for translated text that also goes to the logs.
- Use `notify_error!("title-id", "message-id", argument = value)` for errors. It shows the notification in the user's language and logs the message in English.
