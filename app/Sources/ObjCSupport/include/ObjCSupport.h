#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

@interface VTTObjC : NSObject

/// Runs `block` and turns an Objective-C exception into a Swift error. AVAudioEngine raises
/// NSExceptions (e.g. from installTap when the device format changes mid-call), which Swift
/// cannot catch and which would otherwise crash the app.
+ (BOOL)catchException:(NS_NOESCAPE void (^)(void))block error:(NSError * _Nullable * _Nullable)error;

@end

NS_ASSUME_NONNULL_END
