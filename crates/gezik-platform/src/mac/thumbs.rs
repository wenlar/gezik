//! macOS: Quick Look thumbnails (PDF, HEIC, video, Pages, PSD …), as Finder shows them.

use std::path::Path;
use std::sync::mpsc::sync_channel;
use std::time::Duration;

use block2::RcBlock;
use objc2::AllocAnyThread;
use objc2::rc::autoreleasepool;
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSError, NSString, NSURL};
use objc2_quick_look_thumbnailing::{
    QLThumbnailGenerationRequest, QLThumbnailGenerationRequestRepresentationTypes, QLThumbnailGenerator,
    QLThumbnailRepresentation,
};

use crate::Rgba;

/// The longest one thumbnail may take (a long video's); then it is cancelled.
const LIMIT: Duration = Duration::from_secs(10);
/// How often a waiting thumbnail asks whether it is still wanted.
const STEP: Duration = Duration::from_millis(50);

pub fn thumbnail(path: &Path, px: u32, wanted: &dyn Fn() -> bool) -> Option<Rgba> {
    // `picture::thumbnail_while` has already skipped files only in the cloud.
    let text = path.to_str()?;
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(text));
        let side = f64::from(px.clamp(1, 1024));
        // SAFETY: a file URL, a positive size, scale 1 (`px` is already in pixels).
        let request = unsafe {
            QLThumbnailGenerationRequest::initWithFileAtURL_size_scale_representationTypes(
                QLThumbnailGenerationRequest::alloc(),
                &url,
                CGSize { width: side, height: side },
                1.0,
                QLThumbnailGenerationRequestRepresentationTypes::Thumbnail,
            )
        };
        let (send, receive) = sync_channel::<Option<Rgba>>(1);
        // Called once, on one of Quick Look's queues; the picture is converted there.
        let done = RcBlock::new(move |representation: *mut QLThumbnailRepresentation, _error: *mut NSError| {
            // SAFETY: Quick Look passes a live representation, or null with an error.
            let image = unsafe { representation.as_ref() }
                .and_then(|representation| super::image::cg_to_rgba(&*unsafe { representation.CGImage() }, px));
            let _ = send.try_send(image);
        });
        // SAFETY: the shared generator lives for the process; the request and block are retained by it.
        let generator = unsafe { QLThumbnailGenerator::sharedGenerator() };
        unsafe { generator.generateBestRepresentationForRequest_completionHandler(&request, &done) };
        match crate::picture::wait_while(&receive, wanted, LIMIT, STEP) {
            Some(image) => image,
            None => {
                // SAFETY: the request this generator was given.
                unsafe { generator.cancelRequest(&request) };
                None
            }
        }
    })
}
