# Archived: Sign in with Apple

Dropped for good on 2026-11-24. The owner left the Apple Developer Program on
2026-11-18, and real Sign in with Apple needs a paid membership (App ID
capability, provisioning profile, Developer ID signing). It was never active:
every build was ad-hoc signed, so the helper always fell back to a per-machine
id.

What replaced it:

- **macOS app:** `scripts/app/launcher.sh` now creates and reads the
  per-machine id itself: the same file
  (`~/Library/Application Support/Antediluvia/local_account_id`) and the same
  `local-<uuid>` format the helper used, so existing local players keep their
  characters. `scripts/make_app.sh` no longer compiles a helper.
- **Browser:** unchanged in kind: a random id in `localStorage`
  (`antediluvia_id`). New ids are 128-bit (`web_` + 32 hex); older 32-bit ids
  are kept.
- **Wire and DB:** the field and column are still called `apple_id`. It is an
  opaque account id, and nothing checks it for an Apple shape; renaming it would
  break existing clients and saved accounts for no gain.

Files here, kept as history and not built: `AppleSignIn.swift` (the helper),
`applesignin.entitlements`, `APPLE_SIGN_IN.md` (the owner steps that no
longer apply).
