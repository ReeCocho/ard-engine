use std::collections::VecDeque;

use ard_formats::texture::MipType;
use ard_pal::prelude::*;
use ard_render_base::resource::{ResourceAllocator, ResourceId};
use ard_render_meshes::factory::{MeshFactory, MeshUpload};
use ard_render_textures::{
    factory::{TextureFactory, TextureMipUpload, TextureUpload},
    texture::TextureResource,
};

// TODO: Make this configurable.
const UPLOAD_BUDGET: u64 = 4 * 1024 * 1024;

pub(crate) struct Staging {
    ctx: Context,
    uploads: Vec<Upload>,
    pending: VecDeque<StagingRequest>,
}

pub(crate) enum StagingRequest {
    Mesh {
        id: ResourceId,
        upload: MeshUpload,
    },
    Texture {
        id: ResourceId,
        upload: TextureUpload,
    },
    TextureMip {
        id: ResourceId,
        upload: TextureMipUpload,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum StagingResource {
    StaticMesh(ResourceId),
    Texture { id: ResourceId, loaded_mips: u32 },
    TextureMip { id: ResourceId, mip_level: u32 },
}

struct Upload {
    /// The job on the transfer queue to wait on.
    transfer_job: Job,
    /// The optional job on the main queue to wait on.
    main_job: Option<Job>,
    /// The list of resources that will be uploaded once the job is complete.
    resources: Vec<StagingResource>,
}

struct UploadCommands<'a> {
    ctx: Context,
    transfer: CommandBuffer<'a>,
    main: Option<CommandBuffer<'a>>,
    resources: Vec<StagingResource>,
}

impl Staging {
    pub fn new(ctx: Context) -> Self {
        Staging {
            ctx,
            uploads: Vec::default(),
            pending: VecDeque::default(),
        }
    }

    pub fn add(&mut self, request: StagingRequest) {
        self.pending.push_back(request);
    }

    /// Checks if any uploads are complete. Runs a closure for each resource that is complete.
    pub fn flush_complete_uploads(
        &mut self,
        blocking: bool,
        mut on_complete: impl FnMut(StagingResource),
    ) {
        // TODO: When drain filter gets put into stable, this can all be done in one function chain
        let mut to_remove = Vec::default();
        loop {
            for (i, upload) in self.uploads.iter_mut().enumerate() {
                if let Some(main_job) = &upload.main_job {
                    if main_job.poll_status() == JobStatus::Running {
                        continue;
                    }
                }
                if upload.transfer_job.poll_status() == JobStatus::Complete {
                    to_remove.push(i);
                    for resource in &upload.resources {
                        on_complete(*resource);
                    }
                }
            }

            if !(blocking && to_remove.len() != self.uploads.len()) {
                break;
            }
        }

        // Removes finished commands
        to_remove.sort_unstable();
        for i in to_remove.into_iter().rev() {
            self.uploads.swap_remove(i);
        }
    }

    /// Begin pending uploads.
    pub fn upload(
        &mut self,
        mesh_factory: &mut MeshFactory,
        textures: &ResourceAllocator<TextureResource>,
    ) {
        if self.pending.is_empty() {
            return;
        }

        let mut commands = UploadCommands::new(self.ctx.clone());

        let mut upload_count = 0;
        let mut upload_size = 0;

        for request in &self.pending {
            // TODO: This is needed to prevent stalling the GPU with massive requests. It should be
            // modified to take into account upload sizes since that's really what the killer is.
            if upload_count != 0 && upload_size >= UPLOAD_BUDGET {
                break;
            }

            upload_count += 1;
            upload_size += request.upload_size();

            let resc = match request {
                StagingRequest::Mesh { id, upload } => {
                    mesh_factory.upload(commands.transfer(), upload);
                    StagingResource::StaticMesh(*id)
                }
                StagingRequest::Texture { id, upload } => {
                    let texture = match textures.get(*id) {
                        Some(texture) => texture,
                        // Texture was dropped so upload is no longer needed
                        None => continue,
                    };

                    match upload.mip_type {
                        MipType::Generate => TextureFactory::upload_gen_mip(
                            commands.main(),
                            &texture.texture,
                            texture.mip_levels,
                            upload,
                        ),
                        MipType::Upload(_, _) => TextureFactory::upload(
                            commands.transfer(),
                            &texture.texture,
                            texture.mip_levels.saturating_sub(1),
                            upload,
                        ),
                    }
                    StagingResource::Texture {
                        id: *id,
                        loaded_mips: upload.loaded_mips,
                    }
                }
                StagingRequest::TextureMip { id, upload } => {
                    let texture = match textures.get(*id) {
                        Some(texture) => texture,
                        // Texture was dropped so upload is no longer needed
                        None => continue,
                    };

                    TextureFactory::upload_mip(commands.transfer(), &texture.texture, upload);
                    StagingResource::TextureMip {
                        id: *id,
                        mip_level: upload.mip_level,
                    }
                }
            };

            commands.add_resource(resc);
        }

        // Submit the job
        self.uploads.push(commands.submit());

        // Clear processed staging requests
        self.pending.drain(..upload_count);
    }
}

impl<'a> UploadCommands<'a> {
    pub fn new(ctx: Context) -> Self {
        Self {
            transfer: ctx.transfer().command_buffer(),
            main: None,
            ctx,
            resources: Vec::default(),
        }
    }

    pub fn transfer(&mut self) -> &mut CommandBuffer<'a> {
        &mut self.transfer
    }

    pub fn main(&mut self) -> &mut CommandBuffer<'a> {
        if self.main.is_none() {
            self.main = Some(self.ctx.main().command_buffer());
        }
        self.main.as_mut().unwrap()
    }

    pub fn add_resource(&mut self, resource: StagingResource) {
        self.resources.push(resource);
    }

    pub fn submit(self) -> Upload {
        Upload {
            transfer_job: self
                .ctx
                .transfer()
                .submit_async(Some("transfer_staging"), self.transfer),
            main_job: self
                .main
                .map(|cb| self.ctx.main().submit(Some("main_staging"), cb)),
            resources: self.resources,
        }
    }
}

impl StagingRequest {
    #[inline(always)]
    pub fn upload_size(&self) -> u64 {
        match self {
            StagingRequest::Mesh { upload, .. } => {
                upload.index_staging.size() + upload.vertex_staging.size()
            }
            StagingRequest::Texture { upload, .. } => upload.staging.size(),
            StagingRequest::TextureMip { upload, .. } => upload.staging.size(),
        }
    }
}
