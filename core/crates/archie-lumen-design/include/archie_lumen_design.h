#ifndef ARCHIE_LUMEN_DESIGN_H
#define ARCHIE_LUMEN_DESIGN_H
#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>
// XaviCottier source design, Rust-owned decisions. Qt is an upload/lifecycle host.
#ifdef __cplusplus
extern "C" {
#endif
typedef struct DesignVec3 { float x,y,z; } DesignVec3;
typedef struct DesignBody { DesignVec3 position,velocity,anchor; uint32_t fixed; } DesignBody;
typedef struct DesignMotion { float radius,spring,damping,reach,reaction; } DesignMotion;
// Static null-terminated UTF-8 {schema,defaults,profiles:[{id,name,overrides}],legacy,
// presets:[{id,name,description,settings}]}: profiles are main.js's field/crown/atlas/dense
// and legacy the "Antes" look, both as overrides of defaults.
const char *design_presets_json(void);
// Style IDs: 0 original,1 prism,2 obsidian,3 titanium,4 hologram,5 crystal,
// 6 circuit,7 photon,8 ceramic. Invalid arguments return 0/false, never unwind.
float design_height(uint32_t style,float u,float v,float frequency);
// Linear RGBA8 albedo (source GPU bake), linear RGB tangent normal+A roughness.
// Each writable nonoverlapping output buffer has >=buffer_len bytes; size in 1..4096.
bool design_surface(uint32_t style,uint32_t size,float frequency,uint8_t *albedo,uint8_t *surface,size_t buffer_len);
// Result bit1=positions changed, bit2=simulation active. Pointer null means no focus.
// Bodies writable, all other pointers readable; nonoverlapping, aligned and valid.
uint32_t design_step(DesignBody *bodies,size_t count,const DesignVec3 *pointer,float dt,const DesignMotion *motion);
// Query: vertices=null,indices=null,capacities=0. Counts are vertices/indices.
// Fill: vertex_capacity is FLOAT count (8*vertex_count), index_capacity is u32 count.
// Vertex layout: position3,normal3,uv2. Style0 and style8 return false: original mesh retained.
bool design_mesh(uint32_t style,bool folder,float *vertices,size_t vertex_capacity,uint32_t *indices,size_t index_capacity,size_t *out_vertex_count,size_t *out_index_count);
// Quality IDs: 0 original/quality,1 balanced,2 performance.
uint32_t design_texture_size(uint32_t max_resolution,uint32_t max_capability,uint32_t quality,float projected_size,uint32_t current);
bool design_hybrid(uint32_t style,float depth,float emissive,float contrast,float *output,size_t len);
typedef struct DesignShellNode { DesignVec3 position,color; uint32_t branch; } DesignShellNode;
typedef struct DesignShellInput {
    uint32_t kind; const DesignShellNode *nodes; size_t count;
    float radius,density,scale; DesignVec3 warm,cool,tint;
} DesignShellInput;
// Kinds:0 fragments,1 veil,2 filaments,3 aurora. Observed nodes <=420.
// Query null/zero buffers first. Vertex capacity counts floats, stride10 pos/normal/RGBA.
// Caller provides distinct aligned input/output allocations; Rust retains nothing.
bool design_shell(const DesignShellInput *input,float *vertices,size_t vertex_capacity,
    uint32_t *indices,size_t index_capacity,size_t *vertex_count,size_t *index_count,float *radius);
// Content sphere (Xavi's desktop): read-only, bounded listing of a folder laid out by
// field.js buildField ("regions"). Writes the scene JSON LumenField::accept reads into
// out and returns its length, or 0 if the folder is unreadable or it does not fit.
typedef struct DesignContentOptions {
    uint32_t limit, files_per_folder;
    float category_pull, hierarchy_pull, radial_layers, size_influence, scale;
} DesignContentOptions;
size_t design_content_scene(const char *root, size_t root_len, const DesignContentOptions *options,
    int64_t generated_at_ms, char *out, size_t capacity);

// Xavi's neural sphere (Lumenfield's "Esfera neuronal", API 2): model and scene state in Rust,
// drawn by the host with the original's materials. Opaque handles; Rust keeps no caller buffer.
// Buffers: query with out=NULL and capacity 0 (returns the count), then fill (returns the count
// written, or 0 when the capacity is short). Strings are NUL-terminated UTF-8. Nothing unwinds.
typedef struct NeuralIndex NeuralIndex;
typedef struct NeuralSphere NeuralSphere;
typedef struct NeuralPoint { float position[3], colour[3], kind, fire, ring, born; } NeuralPoint;
typedef struct NeuralVertex { float position[3], colour[3]; } NeuralVertex;
typedef struct NeuralSpark { float position[3], colour[3], fade; } NeuralSpark;
typedef struct NeuralStats {
    uint32_t visible, moving, leaving, born, total, sparks;
    float max_step, dt, time;
    uint64_t rebuilds, revision;
} NeuralStats;
// Line layers: 0 skeleton and axons, 2..6 mesh, bridge and the host's links, 7 the selection.
#define NEURAL_LINE_LAYERS 8
#define NEURAL_HOT_LAYER 7
// Read-only, bounded listing of a folder (safe off the GUI thread). NULL if unreadable.
NeuralIndex *neural_scan(const char *root, size_t root_len, uint32_t limit, uint32_t files_per_folder);
size_t neural_index_count(const NeuralIndex *index);
void neural_index_free(NeuralIndex *index);
// still: the original's deterministic frame (no motion, no build). seed drives the sparks (7).
NeuralSphere *neural_new(const NeuralIndex *index, uint32_t seed, bool still);
void neural_free(NeuralSphere *sphere);
bool neural_set_index(NeuralSphere *sphere, const NeuralIndex *index, bool animate);
void neural_grow(NeuralSphere *sphere, float seconds);
void neural_step(NeuralSphere *sphere, float dt);
float neural_prime_still(NeuralSphere *sphere);
bool neural_building(const NeuralSphere *sphere);
bool neural_stats(const NeuralSphere *sphere, NeuralStats *out);
size_t neural_points(const NeuralSphere *sphere, NeuralPoint *out, size_t capacity);
size_t neural_lines(const NeuralSphere *sphere, uint32_t layer, NeuralVertex *out, size_t capacity);
size_t neural_sparks(const NeuralSphere *sphere, NeuralSpark *out, size_t capacity);
// Decoration as floats (radius 100): 0 ghosts xyz, 1 globe, 2 base, 3 orbit (segments xyz),
// 4 projection cone (triangles, xyz and the cylinder's v per vertex). Capacity counts floats.
size_t neural_decor(uint32_t part, float *out, size_t capacity);
// Live settings by the original's names; invalid values change nothing and return false.
bool neural_set_number(NeuralSphere *sphere, const char *key, double value);
bool neural_set_flag(NeuralSphere *sphere, const char *key, bool value);
bool neural_set_background(NeuralSphere *sphere, const char *hex);
bool neural_set_link(NeuralSphere *sphere, uint32_t kind, int32_t on /* 0, 1, else keep */, double opacity /* NaN keeps */);
size_t neural_settings_json(const NeuralSphere *sphere, char *out, size_t capacity);
bool neural_select(NeuralSphere *sphere, const char *id);
bool neural_activate(NeuralSphere *sphere, const char *id);
bool neural_toggle_fold(NeuralSphere *sphere, const char *id);
size_t neural_pick(const NeuralSphere *sphere, const float origin[3], const float direction[3], float threshold,
    char *out, size_t capacity);
bool neural_position(const NeuralSphere *sphere, const char *id, float out[3]);
// Text queries: call with capacity 0 first (computes and returns the length), then fill.
// Labels: "kind\tx\ty\tz\tid\ttext\n" lines (kind p program, f folder). Selection: id, title,
// counts and folded note, one line each.
size_t neural_labels(NeuralSphere *sphere, char *out, size_t capacity);
size_t neural_selection(NeuralSphere *sphere, const char *root_label, char *out, size_t capacity);
bool neural_link_counts(NeuralSphere *sphere, uint32_t out[7]);
#ifdef __cplusplus
}
#endif
#endif
