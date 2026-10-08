// Signalsmith Stretch を Rust から呼ぶための最小限の C-ABI シム。
// bindgen を使わずに済むよう、テンプレート/クラスは全てここで閉じる。

#include "signalsmith-stretch.h"

#include <cstdint>
#include <new>

namespace {

using Stretch = signalsmith::stretch::SignalsmithStretch<float>;

struct VcStretch {
    Stretch stretch;
    int channels;
    float sample_rate;
};

} // namespace

extern "C" {

// 生成。channels >= 1, sample_rate > 0。失敗時は nullptr。
void *vc_stretch_new(int channels, float sample_rate, int cheaper) {
    if (channels < 1 || !(sample_rate > 0)) return nullptr;
    VcStretch *s = new (std::nothrow) VcStretch{Stretch(0L), channels, sample_rate};
    if (!s) return nullptr;
    if (cheaper) {
        s->stretch.presetCheaper(channels, sample_rate, false);
    } else {
        s->stretch.presetDefault(channels, sample_rate, false);
    }
    return s;
}

void vc_stretch_free(void *p) {
    delete static_cast<VcStretch *>(p);
}

void vc_stretch_reset(void *p) {
    static_cast<VcStretch *>(p)->stretch.reset();
}

// transpose/formant は半音、formant_base_hz は 0 で自動検出、tonality_limit_hz は 0 で無効。
void vc_stretch_set(void *p, float transpose_semitones, float formant_semitones,
                    float formant_base_hz, float tonality_limit_hz, int compensate_pitch) {
    VcStretch *s = static_cast<VcStretch *>(p);
    float tonality = tonality_limit_hz > 0 ? tonality_limit_hz / s->sample_rate : 0.0f;
    s->stretch.setTransposeSemitones(transpose_semitones, tonality);
    s->stretch.setFormantSemitones(formant_semitones, compensate_pitch != 0);
    // setFormantBase はサンプルレートに対する比で受け取る(README: setFormantBase(200/sampleRate))
    s->stretch.setFormantBase(formant_base_hz > 0 ? formant_base_hz / s->sample_rate : 0.0f);
}

int vc_stretch_input_latency(const void *p) {
    return static_cast<const VcStretch *>(p)->stretch.inputLatency();
}

int vc_stretch_output_latency(const void *p) {
    return static_cast<const VcStretch *>(p)->stretch.outputLatency();
}

// in/out はチャンネルごとのポインタ配列(channels 本)。
void vc_stretch_process(void *p, const float *const *in, int n_in, float *const *out, int n_out) {
    VcStretch *s = static_cast<VcStretch *>(p);
    s->stretch.process(in, n_in, out, n_out);
}

// 入力を与えずに残りの出力を取り出す。
void vc_stretch_flush(void *p, float *const *out, int n_out, float playback_rate) {
    VcStretch *s = static_cast<VcStretch *>(p);
    s->stretch.flush(out, n_out, playback_rate);
}

} // extern "C"
