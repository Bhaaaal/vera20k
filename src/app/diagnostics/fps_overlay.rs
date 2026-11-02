//! Final-window text overlays: the FPS readout (hidden at every launch until
//! Ctrl+Alt+F or the Options page's FPS Counter box shows it), the network
//! status line, and the test HUD's bottom-left block.
//! Independent of tactical cameras, optional dev overlays, retail fonts, and
//! shell/offscreen composition.

use crate::util::time::Instant;
use std::sync::Mutex;

use crate::render::batch::{BatchRenderer, CameraUniform, InstanceBufferPool};
use crate::render::bit_font::BitFont;
use crate::render::gpu::GpuContext;

use super::state::FrameTimer;

/// Left inset of every window-fixed text block, in window pixels.
const WINDOW_LEFT: f32 = 8.0;
/// Baseline distance between two lines of window-fixed text.
const HUD_LINE_HEIGHT: f32 = 20.0;
/// Gap between the window's bottom edge and a bottom-anchored block; the test
/// HUD's block is the only thing that anchors to the bottom edge.
#[cfg(feature = "dev-overlays")]
const HUD_BOTTOM_MARGIN: f32 = 12.0;

pub(crate) struct FpsOverlay {
    visible: bool,
    font: BitFont,
    camera_buffer: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    frame: Mutex<OverlayFrame>,
}

struct OverlayFrame {
    timer: FrameTimer,
    pool: InstanceBufferPool,
    size: [u32; 2],
    depth: wgpu::TextureView,
    logged_first_frame: bool,
    simulation: Option<crate::app::presentation::interpolation::SimulationReadout>,
}

impl FpsOverlay {
    pub(crate) fn new(gpu: &GpuContext, batch: &BatchRenderer) -> Self {
        let (camera_buffer, camera_group) = batch.create_ui_camera(&gpu.device);
        Self {
            visible: false,
            font: BitFont::fallback_5x7(&gpu.device, &gpu.queue, batch),
            camera_buffer,
            camera_group,
            frame: Mutex::new(OverlayFrame {
                timer: FrameTimer::new(),
                pool: InstanceBufferPool::new(),
                size: [gpu.config.width, gpu.config.height],
                depth: gpu.create_depth_texture(),
                logged_first_frame: false,
                simulation: None,
            }),
        }
    }

    pub(crate) fn set_simulation_readout(
        &self,
        simulation: Option<crate::app::presentation::interpolation::SimulationReadout>,
    ) {
        self.frame
            .lock()
            .expect("FPS overlay frame lock")
            .simulation = simulation;
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    /// The Options page's FPS Counter box.
    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Ctrl+Alt+F: show or hide the readout; returns whether it is now shown.
    pub(crate) fn toggle(&mut self) -> bool {
        self.visible = !self.visible;
        self.visible
    }

    /// Call exactly once after all content has reached the acquired window
    /// surface, including synchronous loading repaints, before submission.
    /// The dedicated camera avoids rewriting uniforms used by earlier passes.
    pub(crate) fn encode(
        &self,
        gpu: &GpuContext,
        batch: &BatchRenderer,
        encoder: &mut wgpu::CommandEncoder,
        destination: &wgpu::Texture,
    ) {
        let mut frame = self.frame.lock().expect("FPS overlay frame lock");
        // Keep measuring while hidden, so the first shown frame has a rate.
        frame.timer.sample(Instant::now());
        if !self.visible {
            return;
        }
        let text = simulation_readout_text(frame.timer.frame_ms_mean(), frame.simulation);
        self.encode_text(
            gpu,
            batch,
            encoder,
            destination,
            &mut frame,
            &text,
            [WINDOW_LEFT, 8.0],
            u32::MAX,
            "window_fps",
        );
    }

    /// Draw one block of text against the window's bottom-left corner, in
    /// window pixels, through the same font, camera and pass the readouts use.
    ///
    /// The block grows upward from `HUD_BOTTOM_MARGIN`, so the lines the caller
    /// adds later never move the ones already shown. Nothing here reads the
    /// camera: the overlay keeps its size and place at every zoom, which is what
    /// the test HUD wants.
    #[cfg(feature = "dev-overlays")]
    pub(crate) fn encode_bottom_left(
        &self,
        gpu: &GpuContext,
        batch: &BatchRenderer,
        encoder: &mut wgpu::CommandEncoder,
        destination: &wgpu::Texture,
        text: &str,
        key: &'static str,
    ) {
        let width = destination.width();
        let height = destination.height();
        let wrap = width.saturating_sub(2 * WINDOW_LEFT as u32);
        // Line count decides the block's top; the encoder re-lays the same text.
        let lines = self.font.wrap_layout(text, wrap).lines.len() as f32;
        let top = ((height as f32) - HUD_BOTTOM_MARGIN - lines * HUD_LINE_HEIGHT).max(0.0);
        let mut frame = self.frame.lock().expect("FPS overlay frame lock");
        self.encode_text(
            gpu,
            batch,
            encoder,
            destination,
            &mut frame,
            text,
            [WINDOW_LEFT, top],
            wrap,
            key,
        );
    }

    /// Persistent replay/recovery facts share the final-window font/camera.
    /// This call neither samples nor changes the FPS counter's visibility.
    pub(crate) fn encode_status(
        &self,
        gpu: &GpuContext,
        batch: &BatchRenderer,
        encoder: &mut wgpu::CommandEncoder,
        destination: &wgpu::Texture,
        text: &str,
    ) {
        let mut frame = self.frame.lock().expect("window status overlay frame lock");
        let wrap = destination.width().saturating_sub(2 * WINDOW_LEFT as u32) / 2;
        self.encode_text(
            gpu,
            batch,
            encoder,
            destination,
            &mut frame,
            &text.to_ascii_uppercase(),
            [WINDOW_LEFT, if self.visible { 30.0 } else { 8.0 }],
            wrap,
            "window_status",
        );
    }

    fn encode_text(
        &self,
        gpu: &GpuContext,
        batch: &BatchRenderer,
        encoder: &mut wgpu::CommandEncoder,
        destination: &wgpu::Texture,
        frame: &mut OverlayFrame,
        text: &str,
        start: [f32; 2],
        wrap: u32,
        key: &'static str,
    ) {
        let size = [destination.width(), destination.height()];
        if frame.size != size {
            frame.depth = gpu.create_depth_texture();
            frame.size = size;
        }
        let camera = CameraUniform {
            screen_size: [size[0] as f32, size[1] as f32],
            camera_pos: [0.0; 2],
            zoom: 1.0,
            world_origin_y: 0.0,
            world_height: 1.0,
            _pad: 0.0,
            native_z_origin_y: 0.0,
            _native_z_pad: 0.0,
        };
        gpu.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));
        let mut instances = Vec::with_capacity(text.len() * 5);
        let layout = self.font.wrap_layout(text, wrap);
        for (line_index, line) in layout.lines.iter().enumerate() {
            for (dx, dy, tint) in [
                (-1.0, 0.0, [0.0; 3]),
                (1.0, 0.0, [0.0; 3]),
                (0.0, -1.0, [0.0; 3]),
                (0.0, 1.0, [0.0; 3]),
                (0.0, 0.0, [1.0, 1.0, 0.25]),
            ] {
                instances.extend(self.font.build_text(
                    &text[line.start_byte..line.end_byte],
                    start[0] + dx,
                    start[1] + line_index as f32 * HUD_LINE_HEIGHT + dy,
                    2.0,
                    0.0,
                    tint,
                    [0.0; 2],
                ));
            }
        }
        frame.pool.upload(gpu, key, &instances);
        if !frame.logged_first_frame {
            log::info!(
                "{key}: final window surface {}x{}, {} glyph quads",
                size[0],
                size[1],
                instances.len()
            );
            frame.logged_first_frame = true;
        }
        let view = destination.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Window FPS"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &frame.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some((buffer, count)) = frame.pool.get(key) {
            batch.draw_passthrough_range_bound(
                &mut pass,
                self.font.atlas(),
                buffer,
                &self.camera_group,
                0,
                count,
            );
        }
    }
}

fn readout_text(frame_ms: f32) -> String {
    if !frame_ms.is_finite() || frame_ms <= 0.0 {
        "FPS --".to_owned()
    } else {
        // The built-in font is uppercase-only and has no decimal point.
        format!("FPS {:.0}  {:.0} MS", 1000.0 / frame_ms, frame_ms)
    }
}

fn simulation_readout_text(
    frame_ms: f32,
    simulation: Option<crate::app::presentation::interpolation::SimulationReadout>,
) -> String {
    let mut text = readout_text(frame_ms);
    if let Some(simulation) = simulation {
        text.push_str(&format!(
            "  SIM {:.0} TPS  INTERP {}",
            simulation.tps,
            if simulation.interpolation && simulation.sampling {
                "ON"
            } else if simulation.interpolation {
                "BYPASS"
            } else {
                "OFF"
            },
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_overlay_distinguishes_present_rate_from_logic_and_interpolation() {
        let readout = crate::app::presentation::interpolation::SimulationReadout {
            tps: 30.0,
            interpolation: true,
            sampling: true,
        };
        assert_eq!(
            simulation_readout_text(1000.0 / 60.0, Some(readout)),
            "FPS 60  17 MS  SIM 30 TPS  INTERP ON"
        );
        assert!(
            simulation_readout_text(
                16.0,
                Some(crate::app::presentation::interpolation::SimulationReadout {
                    sampling: false,
                    ..readout
                })
            )
            .ends_with("INTERP BYPASS")
        );
        let supported = crate::render::bit_font::fallback_5x7_glyphs();
        for (interpolation, sampling) in [(true, true), (false, false), (true, false)] {
            for ch in simulation_readout_text(
                16.0,
                Some(crate::app::presentation::interpolation::SimulationReadout {
                    interpolation,
                    sampling,
                    ..readout
                }),
            )
            .chars()
            {
                assert!(
                    supported.iter().any(|(glyph, _)| *glyph == ch),
                    "missing {ch}"
                );
            }
        }
    }

    #[test]
    fn fps_overlay_has_a_visible_label_before_measurement() {
        for value in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(readout_text(value), "FPS --");
        }
    }

    #[test]
    fn fps_overlay_reports_render_rate_not_simulation_ticks() {
        assert_eq!(readout_text(1000.0 / 60.0), "FPS 60  17 MS");
        assert_eq!(readout_text(100.0), "FPS 10  100 MS");
    }

    #[test]
    fn fps_overlay_font_covers_every_readout_character() {
        let supported = crate::render::bit_font::fallback_5x7_glyphs();
        for frame_ms in [0.0, 0.5, 1000.0 / 60.0, 100.0, 1234.0] {
            for ch in readout_text(frame_ms).chars() {
                assert!(
                    supported.iter().any(|(glyph, _)| *glyph == ch),
                    "missing {ch}"
                );
            }
        }
    }
}
