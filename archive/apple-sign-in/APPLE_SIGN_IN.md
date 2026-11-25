# Sign in with Apple: what exists, and the step only the owner can do

Status 2026-11-23: the login flow is in place and unchanged by the character
builder work. Real Sign in with Apple is **not active**: every build so far is
ad-hoc signed (`codesign -s -` in `scripts/make_app.sh`), and Apple only
honours the entitlement on a bundle signed by a paid developer team with a
matching provisioning profile. Nothing in this repository can supply that.

## What happens today
1. `scripts/app/launcher.sh` runs the bundled helper `apple-signin`
   (`scripts/app/AppleSignIn.swift`, compiled by `scripts/make_app.sh`).
2. The helper asks `ASAuthorizationAppleIDProvider` for an Apple ID
   credential. With no entitlement the request fails at once and the helper
   prints a per-machine id (`local-<uuid>`, kept in
   `~/Library/Application Support/Antediluvia/local_account_id`).
3. The launcher passes that id to the client as its first argument; the
   client sends it as `apple_id` in `Login`.
4. An account with no character gets `LoginRejected: no character on this
   account`, and the client opens the character builder
   (`crates/client-bevy/src/look/builder.rs`). Create sends `Login` again with
   the `create` payload: name, class, lineage, outfit and the packed look.
5. The server validates the name (`antediluvia_sim::names::validate_name`),
   clamps the look (`Look::sanitize`), saves the character against the id.

So a player keeps one character per machine today, and would keep one per
Apple ID once the entitlement is live. No code change is needed for the switch:
the helper already caches Apple's stable `user` id in `apple_user_id` and
prefers it.

## What the owner must do (Apple developer portal)
Requires an active, paid Apple Developer Program membership on the team that
will sign the app. Bundle id: `org.batesai.antediluvia`.

1. **Identifiers.** developer.apple.com, Certificates, Identifiers & Profiles,
   Identifiers. Create (or open) the App ID `org.batesai.antediluvia`, platform
   macOS, and tick **Sign in with Apple** (enable as a primary App ID). Save.
2. **Certificate.** Make sure the team has a **Developer ID Application**
   certificate and that it is in the login keychain of the build machine.
3. **Profile.** Profiles, new profile, **Developer ID** (macOS, direct
   distribution), App ID `org.batesai.antediluvia`. Download it. It must be
   placed in the bundle as `Antediluvia.app/Contents/embedded.provisionprofile`.
4. **Sign with the entitlement.** Replace the ad-hoc signing in
   `scripts/make_app.sh` with the Developer ID identity, hardened runtime, and
   `scripts/app/applesignin.entitlements`. That file currently holds only
   `com.apple.developer.applesignin`; for a Developer ID build it also needs
   `com.apple.application-identifier` (`<TEAMID>.org.batesai.antediluvia`) and
   `com.apple.developer.team-identifier` (`<TEAMID>`), both matching the
   profile. Then notarize and staple as for the other apps.
5. **Test.** First launch should show Apple's sheet; afterwards
   `~/Library/Application Support/Antediluvia/apple_user_id` exists and the
   client log shows that id instead of `local-...`.

## Known gaps to close when doing the above (not done, not tested)
- The helper is a bare executable in `Contents/Resources`. The entitlement is
  checked against the signed bundle and its profile; a helper outside
  `Contents/MacOS`, or one with no window to anchor the sheet, may be refused.
  If it is, the fix is to make the helper a small agent app (its own bundle id
  under the same App ID prefix, `LSUIElement`), or to call
  AuthenticationServices from the main executable. This cannot be tried
  without the profile.
- The server trusts the id the client sends. For a public shard it should take
  Apple's `identityToken` (a JWT) from the helper and verify it against Apple's
  public keys before accepting the id. Today anyone who knows an id can log in
  as it. The dev-account allowlist has the same weakness.
- The browser client has no Apple login: it keeps a random id in localStorage.
  That needs a Services ID, a verified domain and return URL in the same
  portal, and the token check above.
- Existing local characters are keyed by `local-<uuid>` (or the old `$USER`).
  Moving to a real Apple id needs a one-time "claim my old character" step, or
  those characters are orphaned.
