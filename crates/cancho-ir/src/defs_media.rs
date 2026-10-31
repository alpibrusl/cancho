//! Edition 12's device types (`docs/camera.md` §6, `docs/audio.md` §6): the camera, microphone and speaker
//! capabilities, the stream each opens, and what opening one answers. Split out of `defs.rs`, which is held to the
//! repository's 2,000-line budget.

use crate::defs::{prelude_enum, resource};
use crate::*;

/// The interned names and declared ids `media_types` needs, gathered by `prelude_types`.
pub(crate) struct MediaNames {
    pub(crate) camera: Symbol,
    pub(crate) cam: Symbol,
    pub(crate) camera_opened: Symbol,
    pub(crate) mic: Symbol,
    pub(crate) capture: Symbol,
    pub(crate) mic_opened: Symbol,
    pub(crate) speaker: Symbol,
    pub(crate) playback: Symbol,
    pub(crate) speaker_opened: Symbol,
    pub(crate) camera_def: DefId,
    pub(crate) cam_def: DefId,
    pub(crate) camera_opened_def: DefId,
    pub(crate) mic_def: DefId,
    pub(crate) capture_def: DefId,
    pub(crate) mic_opened_def: DefId,
    pub(crate) speaker_def: DefId,
    pub(crate) playback_def: DefId,
    pub(crate) speaker_opened_def: DefId,
    pub(crate) prefix: Symbol,
    pub(crate) ok_arm: Symbol,
    pub(crate) failed_arm: Symbol,
}

/// The nine types, in the order `PRELUDE_CAMERA` .. `PRELUDE_SPEAKER_OPENED` fix: each capability is indexed by a
/// device-path prefix as `Tty` is, each stream is `Port`'s shape (`res`, one descriptor leaf), and each answer is
/// `TtyOpened`'s (`Failed(errno)` is the program's question, not the compiler's).
pub(crate) fn media_types(n: MediaNames, span: Span) -> Vec<TypeDef> {
    let answer = |name, def, stream_def| TypeDef {
        since: 12,
        ..prelude_enum(
            name,
            def,
            vec![
                (n.ok_arm, vec![Type::Named(stream_def, Vec::new())]),
                (n.failed_arm, vec![Type::Int]),
            ],
            span,
        )
    };
    vec![
        resource(n.camera, n.camera_def, vec![n.prefix], span, 12),
        resource(n.cam, n.cam_def, Vec::new(), span, 12),
        answer(n.camera_opened, n.camera_opened_def, n.cam_def),
        resource(n.mic, n.mic_def, vec![n.prefix], span, 12),
        resource(n.capture, n.capture_def, Vec::new(), span, 12),
        answer(n.mic_opened, n.mic_opened_def, n.capture_def),
        resource(n.speaker, n.speaker_def, vec![n.prefix], span, 12),
        resource(n.playback, n.playback_def, Vec::new(), span, 12),
        answer(n.speaker_opened, n.speaker_opened_def, n.playback_def),
    ]
}
