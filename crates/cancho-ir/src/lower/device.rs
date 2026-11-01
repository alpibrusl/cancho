//! `camera_open`, `mic_open` and `speaker_open`'s lowering (`docs/camera.md` §3, `docs/audio.md` §3): `tty_open`'s
//! shape for the three device capabilities. The capability's prefix travels with the call, checked against the path
//! at run time the way `open_file`'s is, and the row records it.

use crate::*;

impl FnLowering<'_> {
    /// `camera_open(camera, path) -> [camera_open(prefix)] CameraOpened`, and likewise for `mic_open` and
    /// `speaker_open`: the capability names a device-path prefix; the device is opened under it, nonblocking.
    pub(crate) fn device_open(
        &mut self,
        device: Device,
        args: &[ExprId],
        span: Span,
    ) -> Result<(Expr, Type), Diagnostic> {
        let name = device.label();
        let [capability, path] = args else {
            return Err(Diagnostic::new(
                Rule::ArityMismatch,
                format!(
                    "`{name}` takes 2 arguments -- the capability and the path -- but {} were given",
                    args.len()
                ),
                span,
            ));
        };
        let capability_span = self.ast.expr_span(*capability);
        let (value, ty) = self.expr(*capability)?;
        let prefix = self.granted_device_prefix(device, &ty, capability_span)?;
        let bytes = Type::Ref {
            unique: false,
            region: self.unifier.fresh_region(),
            inner: Box::new(Type::Slice(Box::new(Type::Byte))),
        };
        let path_span = self.ast.expr_span(*path);
        let (path_value, path_ty) = self.expr(*path)?;
        self.expect_type(&bytes, &path_ty, path_span)?;
        // The row names the prefix, so the report can say which devices a program may reach.
        self.performed.union(&Effects::new([Label {
            name: name.to_owned(),
            argument: Some(prefix.clone()),
        }]));
        let answer = match device {
            Device::Camera => PRELUDE_CAMERA_OPENED,
            Device::Mic => PRELUDE_MIC_OPENED,
            Device::Speaker => PRELUDE_SPEAKER_OPENED,
        };
        Ok((
            Expr::DeviceOpen { device, prefix, args: vec![value, path_value] },
            Type::Named(self.prelude()[answer], Vec::new()),
        ))
    }

    /// The prefix a borrowed `Camera`, `Mic` or `Speaker` was narrowed to: `granted_tty_prefix`'s shape, refused
    /// with its own message when the argument is not the borrowed capability the verb wants.
    fn granted_device_prefix(
        &mut self,
        device: Device,
        ty: &Type,
        span: Span,
    ) -> Result<String, Diagnostic> {
        let (which, noun) = match device {
            Device::Camera => (PRELUDE_CAMERA, "Camera"),
            Device::Mic => (PRELUDE_MIC, "Mic"),
            Device::Speaker => (PRELUDE_SPEAKER, "Speaker"),
        };
        let resolved = self.unifier.resolve(ty);
        if let Type::Ref { inner, .. } = &resolved
            && let Type::Named(def, args) = self.unifier.resolve(inner)
            && def.0 as usize == which
            && let Some(Type::Lit(prefix)) = args.first()
        {
            return Ok(prefix.clone());
        }
        Err(Diagnostic::new(
            Rule::CapabilityMisused,
            format!(
                "`{}` is not a borrowed `{noun}`; a device is reached through the capability that names the node it may touch",
                self.unifier.display(&resolved)
            ),
            span,
        ))
    }
}
