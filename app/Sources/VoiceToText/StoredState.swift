import SwiftUI

/// SwiftUI's `@State`, written `@StoredState`. From Swift 6.4, `@State` also names a macro whose implementation
/// (SwiftUIMacros) ships only with Xcode, so a Command Line Tools build fails on every `@State`. Through this alias the
/// property wrapper is used directly, as on every earlier toolchain.
typealias StoredState = SwiftUI.State
