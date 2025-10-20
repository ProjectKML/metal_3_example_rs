uint murmur_hash_11(uint src) {
    const uint M = 0x5bd1e995;
    uint h = 1190494759;
    src *= M;
    src ^= src >> 24;
    src *= M;
    h *= M;
    h ^= src;
    h ^= h >> 13;
    h *= M;
    h ^= h >> 15;

    return h;
}

float3 murmur_hash_11_color(uint src) {
    const uint hash = murmur_hash_11(src);
    return float3((float)((hash >> 16) & 0xFF), (float)((hash >> 8) & 0xFF), (float)(hash & 0xFF)) / 256.0;
}

struct Vertex {
    float posX, posY, posZ;
    float texX, texY;
    float nx, ny, nz;
};

struct Meshlet {
    uint data_offset;
    uint vertex_count;
    uint triangle_count;
};

struct MeshOutput {
    float4 position : SV_Position;
    float2 tex_coord : TEXCOORD;
    float3 normal : NORMAL;
    float3 meshlet_color : MESHLET_COLOR;
};

struct PixelInput {
    float2 tex_coord : TEXCOORD;
    float3 normal : NORMAL;
    float3 meshlet_color : MESHLET_COLOR;
};

StructuredBuffer<Vertex> vertices : register(t0, space0);
StructuredBuffer<Meshlet> meshlets : register(t1, space0);
StructuredBuffer<uint> meshlet_data : register(t2, space0);

Texture2D color_texture : register(t3, space0);
SamplerState color_sampler : register(s4, space0);

cbuffer MeshUniforms : register(b0, space0) {
    float4x4 mvp_matrix;
    uint32_t render_type;
};

uint get_index(uint index_offset, uint index) {
    const uint byte_offset = ((index & 3)) << 3;
    return (meshlet_data[index_offset + (index >> 2u)] & (0xFFu << byte_offset)) >> byte_offset;
}

[outputtopology("triangle")]
[numthreads(32, 1, 1)]
void geometry_mesh(out vertices MeshOutput output_vertices[64],
                   out indices uint3 output_triangles[124],
                   uint3 gtid : SV_GroupThreadID,
                   uint3 gid : SV_GroupID) {
    const uint meshlet_index = gid.x >> 5;

    const Meshlet meshlet = meshlets[meshlet_index];

    SetMeshOutputCounts(meshlet.vertex_count, meshlet.triangle_count);

    const float3 meshlet_color = murmur_hash_11_color(meshlet_index);

    for(uint i = gtid.x; i < meshlet.vertex_count; i += 32) {
        const uint vertex_index = meshlet_data[meshlet.data_offset + i];
        const Vertex current_vertex = vertices[vertex_index];

        MeshOutput output;
        output.position = mul(mvp_matrix, float4(current_vertex.posX, current_vertex.posY, current_vertex.posZ, 1.0));
        output.tex_coord = float2(current_vertex.texX, current_vertex.texY);
        output.normal = float3(current_vertex.nx, current_vertex.ny, current_vertex.nz);
        output.meshlet_color = meshlet_color;

        output_vertices[i] = output;
    }

    const uint num_index_groups = (meshlet.triangle_count * 3 + 3) >> 2;

    for (uint i = gtid.x; i < meshlet.triangle_count; i++) {
        const uint data_offset = meshlet.data_offset + meshlet.vertex_count;
        const uint index_offset = i * 3;

        output_triangles[i] = uint3(get_index(data_offset, index_offset), get_index(data_offset, index_offset + 1), get_index(data_offset, index_offset + 2));
    }
}

float4 geometry_pixel(PixelInput input) : SV_Target0 {
    const float4 color_sample = color_texture.Sample(color_sampler,
        float2(input.tex_coord.x, 1.0 - input.tex_coord.y));

    return render_type == 0 ? color_sample : float4(float3(input.meshlet_color), 1.0);
}