// Antediluvia Sign in with Apple helper.
// Prints a stable account identifier on stdout and exits 0.
//
// Path A (real SIWA): when the bundle is signed with the
// com.apple.developer.applesignin entitlement + provisioning profile, this
// runs ASAuthorizationController and returns Apple's stable `user` id,
// cached in Application Support so later launches skip the sheet.
// Path B (fallback): entitlement missing, user cancels, or any error —
// returns a persistent per-machine UUID (created once). Alpha builds without
// the portal profile still get a stable identity, never $USER.

import AuthenticationServices
import Foundation

let supportDir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    .appendingPathComponent("Antediluvia")
try? FileManager.default.createDirectory(at: supportDir, withIntermediateDirectories: true)
let cachedApple = supportDir.appendingPathComponent("apple_user_id")
let fallbackFile = supportDir.appendingPathComponent("local_account_id")

func finish(_ id: String) -> Never {
    print(id)
    exit(0)
}

func fallbackId() -> String {
    if let id = try? String(contentsOf: fallbackFile, encoding: .utf8)
        .trimmingCharacters(in: .whitespacesAndNewlines), !id.isEmpty {
        return id
    }
    let id = "local-" + UUID().uuidString.lowercased()
    try? id.write(to: fallbackFile, atomically: true, encoding: .utf8)
    return id
}

// Cached real Apple id from a previous successful authorization?
if let id = try? String(contentsOf: cachedApple, encoding: .utf8)
    .trimmingCharacters(in: .whitespacesAndNewlines), !id.isEmpty {
    finish(id)
}

final class Delegate: NSObject, ASAuthorizationControllerDelegate,
    ASAuthorizationControllerPresentationContextProviding {
    func authorizationController(controller: ASAuthorizationController,
                                 didCompleteWithAuthorization auth: ASAuthorization) {
        if let cred = auth.credential as? ASAuthorizationAppleIDCredential {
            try? cred.user.write(to: cachedApple, atomically: true, encoding: .utf8)
            finish(cred.user)
        }
        finish(fallbackId())
    }
    func authorizationController(controller: ASAuthorizationController,
                                 didCompleteWithError error: Error) {
        FileHandle.standardError.write("SIWA unavailable (\(error.localizedDescription)); using local identity\n".data(using: .utf8)!)
        finish(fallbackId())
    }
    func presentationAnchor(for controller: ASAuthorizationController) -> ASPresentationAnchor {
        ASPresentationAnchor()
    }
}

let request = ASAuthorizationAppleIDProvider().createRequest()
request.requestedScopes = []
let controller = ASAuthorizationController(authorizationRequests: [request])
let delegate = Delegate()
controller.delegate = delegate
controller.presentationContextProvider = delegate
controller.performRequests()

// Give the sheet up to 120 s; a cancel/error path exits sooner via Delegate.
RunLoop.main.run(until: Date(timeIntervalSinceNow: 120))
finish(fallbackId())
