use std::{
    fs::File,
    io::{BufReader, BufWriter},
    ops::Div,
    path::PathBuf,
};

use ard_engine::{
    assets::prelude::*,
    ecs::prelude::*,
    formats::texture::{TextureData, TextureHeader},
    render::{prelude::Format, texture::TextureAsset},
};
use camino::Utf8PathBuf;
use image::{imageops::FilterType, GenericImageView, ImageFormat};
use path_macro::path;

use crate::{
    assets::{
        meta::{MetaData, MetaFile, TextureImportSettings, TextureMipSetting},
        op::AssetNameGenerator,
        CurrentAssetPath, EditorAssets,
    },
    gui::util,
};

use super::{EditorTask, TaskConfirmation, TaskState};

pub struct TextureImportTask {
    src_path: PathBuf,
    active_package: PathBuf,
    active_assets: Utf8PathBuf,
    raw_rel_path: Utf8PathBuf,
    new_assets: Vec<AssetNameBuf>,
    old_mips: Vec<PathBuf>,
    import_settings: TextureImportSettings,
    assets: Option<Assets>,
    baked_asset: Utf8PathBuf,
    state: TaskState,
}

impl TextureImportTask {
    pub fn new(path: PathBuf) -> Self {
        Self {
            state: TaskState::new(format!("Import {}", path.display())),
            src_path: path,
            active_assets: Utf8PathBuf::default(),
            old_mips: Vec::default(),
            active_package: PathBuf::default(),
            raw_rel_path: Utf8PathBuf::default(),
            new_assets: Vec::default(),
            assets: None,
            baked_asset: Utf8PathBuf::default(),
            import_settings: TextureImportSettings::default(),
        }
    }

    pub fn reimport(
        src_path: PathBuf,
        baked_asset: Utf8PathBuf,
        import_settings: TextureImportSettings,
    ) -> Self {
        Self {
            state: TaskState::new(format!("Import {}", src_path.display())),
            src_path,
            baked_asset,
            active_assets: Utf8PathBuf::default(),
            active_package: PathBuf::default(),
            raw_rel_path: Utf8PathBuf::default(),
            new_assets: Vec::default(),
            old_mips: Vec::default(),
            assets: None,
            import_settings,
        }
    }

    fn raw_path_rel(&self) -> &Utf8PathBuf {
        &self.raw_rel_path
    }

    fn raw_path_abs(&self) -> Utf8PathBuf {
        let mut p = self.active_assets.clone();
        p.push(self.raw_path_rel());
        p
    }

    fn meta_path_rel(&self) -> Utf8PathBuf {
        let mut p = self.raw_rel_path.clone();
        let ext = p.extension().unwrap_or("");
        p.set_extension(format!("{ext}.meta"));
        p
    }

    fn meta_path_abs(&self) -> Utf8PathBuf {
        let mut p = self.active_assets.clone();
        p.push(self.meta_path_rel());
        p
    }
}

impl EditorTask for TextureImportTask {
    fn has_confirm_ui(&self) -> bool {
        self.baked_asset == Utf8PathBuf::default()
    }

    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> anyhow::Result<TaskConfirmation> {
        ui.label(format!(
            "Do you want to import `{}`?",
            self.src_path.display()
        ));

        if ui.add(util::constructive_button("Yes")).clicked() {
            return Ok(TaskConfirmation::Ready);
        }

        if ui.button("No").clicked() {
            return Ok(TaskConfirmation::Cancel);
        }

        Ok(TaskConfirmation::Wait)
    }

    fn state(&mut self) -> Option<TaskState> {
        Some(self.state.clone())
    }

    fn pre_run(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        let editor_assets = res.get::<EditorAssets>().unwrap();
        let cur_path = res.get::<CurrentAssetPath>().unwrap();

        self.assets = Some(res.get::<Assets>().unwrap().clone());
        self.active_package = editor_assets.active_package_root().into();
        self.active_assets = editor_assets.active_assets_root().into();

        match self.src_path.file_name() {
            Some(file_name) => {
                self.raw_rel_path = cur_path.path().into();
                self.raw_rel_path.push(file_name.to_str().ok_or_else(|| {
                    anyhow::Error::msg("Texture has invalid characters in it's name.")
                })?);
            }
            None => return Err(anyhow::Error::msg("Invalid file name.")),
        }

        if let Some(asset) = editor_assets.find_asset(self.meta_path_rel()) {
            // If the asset is already in this package, mark it for a reimport
            if asset.in_package(editor_assets.active_package_id()) {
                self.baked_asset = asset.meta_file().baked.clone();
                self.import_settings = match asset.meta_file().data {
                    MetaData::Texture(settings) => settings,
                    _ => return Err(anyhow::Error::msg("Wrong asset type.")),
                };
            }

            // If the IDs don't match, we are attempting to shadow
            if asset.package() != editor_assets.active_package_id() {
                return Err(anyhow::Error::msg(
                    "TODO: Texture shadowing is unimplemented.",
                ));
            }
        }

        Ok(())
    }

    fn run(&mut self) -> anyhow::Result<()> {
        let temp_folder = tempfile::TempDir::new()?;
        let assets = self.assets.as_ref().unwrap();

        // Determine the file format
        let file_format = match self
            .src_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase()
            .as_str()
        {
            "jpg" | "jpeg" => ImageFormat::Jpeg,
            "png" => ImageFormat::Png,
            "bmp" => ImageFormat::Bmp,
            "dds" => ImageFormat::Dds,
            "tga" => ImageFormat::Tga,
            "tiff" => ImageFormat::Tiff,
            "webp" => ImageFormat::WebP,
            other => {
                return Err(anyhow::Error::msg(format!(
                    "Unknown texture format `{other:?}`"
                )))
            }
        };

        // Read in the texture
        let file = File::open(&self.src_path)?;
        let reader = BufReader::new(file);
        let image = image::load(reader, file_format)?;

        // Perform compression if required
        let compress = self.import_settings.compress && texture_needs_compression(&image);
        let mip_count = match self.import_settings.mip {
            TextureMipSetting::None => 1,
            TextureMipSetting::GenerateAll => texture_mip_count(&image, compress),
            TextureMipSetting::GenerateExact(desired) => {
                desired.clamp(1, texture_mip_count(&image, compress))
            }
        };

        let format = match (compress, self.import_settings.linear_color_space) {
            (true, true) => Format::BC7Unorm,
            (true, false) => Format::BC7Srgb,
            (false, true) => Format::Rgba8Unorm,
            (false, false) => Format::Rgba8Srgb,
        };

        let step_count = (mip_count + 4) as f32;
        self.state.set_completion(1.0 / step_count);

        let mut name_gen = AssetNameGenerator::new(assets.clone());

        // If a baked asset was provided, we are reimporting.
        let header = if self.baked_asset != Utf8PathBuf::default() {
            let f = File::open(path!(self.active_package / self.baked_asset))?;
            let b = BufReader::new(f);
            let mut header = bincode::deserialize_from::<_, TextureHeader>(b)?;

            let old_mip_count = header.mips.len();

            // If we had fewer mips, create new names
            if old_mip_count < mip_count {
                header
                    .mips
                    .extend((0..(mip_count - old_mip_count)).map(|_| name_gen.generate("")));
            }
            // If we had more mips, remove the old files
            else if old_mip_count > mip_count {
                header.mips.drain(mip_count..).for_each(|mip| {
                    self.old_mips.push(path!(self.active_package / mip));
                    self.new_assets.push(mip);
                });
            }

            header.width = image.width();
            header.height = image.height();
            header.format = format;
            header.sampler = self.import_settings.sampler;

            header
        } else {
            let header = TextureHeader {
                width: image.width(),
                height: image.height(),
                mips: (0..mip_count).map(|_| name_gen.generate("")).collect(),
                format,
                sampler: self.import_settings.sampler,
            };
            self.baked_asset = name_gen.generate(TextureAsset::EXTENSION);

            header
        };

        // Write header to disk
        let f = File::create(path!(temp_folder.path() / self.baked_asset))?;
        let b = BufWriter::new(f);
        bincode::serialize_into(b, &header)?;

        self.new_assets.push(self.baked_asset.clone());
        self.new_assets.extend(name_gen.new_names().iter().cloned());

        self.state.set_completion(2.0 / step_count);

        for mip in 0..mip_count {
            // Resize image based on the mip level
            let (mut width, mut height) = image.dimensions();
            width = (width >> mip).max(1);
            height = (height >> mip).max(1);

            let downsampled = image.resize(width, height, FilterType::Lanczos3);
            let bytes = downsampled.to_rgba8().to_vec();

            // Compress if requested
            if compress {
                let surface = intel_tex_2::RgbaSurface {
                    width,
                    height,
                    stride: width * 4,
                    data: &bytes,
                };
                intel_tex_2::bc7::compress_blocks(
                    &intel_tex_2::bc7::alpha_ultra_fast_settings(),
                    &surface,
                );
            }

            // Save to disk
            let tex_data = TextureData::new(bytes, width, height, format);
            let mip_path = path!(temp_folder.path() / header.mips[mip]);
            let f = File::create(mip_path)?;
            let b = BufWriter::new(f);
            bincode::serialize_into(b, &tex_data)?;

            self.state.set_completion((2.0 + mip as f32) / step_count);
        }

        // Copy raw asset and create meta file
        let raw_dst = self.raw_path_abs();
        if raw_dst != self.src_path {
            std::fs::copy(&self.src_path, raw_dst)?;
        }

        let meta = MetaFile {
            baked: self.baked_asset.clone(),
            data: MetaData::Texture(self.import_settings),
        };
        let f = File::create(self.meta_path_abs())?;
        let b = BufWriter::new(f);
        ron::ser::to_writer(b, &meta)?;

        self.state
            .set_completion((3.0 + mip_count as f32) / step_count);

        // Copy in artifacts
        let folder = temp_folder.into_path();
        fs_extra::dir::move_dir(
            folder,
            &self.active_package,
            &fs_extra::dir::CopyOptions {
                overwrite: true,
                content_only: true,
                ..Default::default()
            },
        )?;
        self.state.set_completion(1.0);

        Ok(())
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        let mut editor_assets = res.get_mut::<EditorAssets>().unwrap();
        let assets = res.get::<Assets>().unwrap();

        self.new_assets.drain(..).for_each(|new_asset| {
            assets.scan_for(&new_asset);
        });

        self.old_mips
            .drain(..)
            .map(|mip| std::fs::remove_file(mip))
            .collect::<Result<_, _>>()?;

        editor_assets.scan_for(self.meta_path_abs())?;

        Ok(())
    }
}

#[inline]
pub fn texture_needs_compression(image: &image::DynamicImage) -> bool {
    let (width, height) = image.dimensions();
    // We only need to compress if our image is at least as big as a block
    width >= 4 && height >= 4
}

#[inline]
fn texture_mip_count(image: &image::DynamicImage, compressed: bool) -> usize {
    let (width, height) = image.dimensions();
    if compressed {
        (width.div(4).max(height.div(4)) as f32).log2() as usize + 1
    } else {
        (width.max(height) as f32).log2() as usize + 1
    }
}
