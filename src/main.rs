use dioxus::prelude::{rsx, use_effect, use_state, Element, Scope};
use rand::Rng;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// ---------- JNI Bridge ----------

use jni::{
    objects::{GlobalRef, JClass, JObject},
    sys::{jfloat, jint},
    JNIEnv,
};

// This is the Java source that must be bundled in APK
static JAVA_CODE: &str = r#"
package com.example.minEq;

import android.media.audiofx.Equalizer;
import android.media.audiofx.Visualizer;
import android.util.Log;

public class GlobalEq {
    private static final String TAG = "GlobalEq";
    private static Equalizer eq = null;
    private static Visualizer visualizer = null;
    private static final int NUM_BANDS = 8;
    private static float[] bandGains = new float[NUM_BANDS];
    private static float globalVol = 0f;

    public static void initGlobalEq() {
        try {
            if (eq != null) {
                eq.release();
                eq = null;
            }
            eq = new Equalizer(0, 0);
            eq.setEnabled(true);
            Log.i(TAG, "Initialized global Equalizer on session 0");
        } catch (Throwable t) {
            Log.e(TAG, "initGlobalEq failed: " + t);
        }
        for (int i = 0; i < NUM_BANDS; i++) {
            bandGains[i] = 0f;
        }
        globalVol = 0f;
        try {
            if (visualizer != null) {
                visualizer.release();
                visualizer = null;
            }
            visualizer = new Visualizer(0);
            int capSize = Visualizer.getCaptureSizeRange()[1];
            visualizer.setCaptureSize(capSize);
            visualizer.setEnabled(true);
            Log.i(TAG, "Visualizer initialized with capture size: " + capSize);
        } catch (Throwable t) {
            Log.e(TAG, "Visualizer init failed: " + t);
        }
    }

    public static void setBandGain(int idx, float gainDb) {
        if (eq == null) return;
        if (idx < 0 || idx >= NUM_BANDS) return;
        bandGains[idx] = gainDb;
        short realBands = eq.getNumberOfBands();
        if (idx < realBands) {
            short mb = (short)(gainDb * 100);
            try {
                eq.setBandLevel((short)idx, mb);
            } catch (Exception e) {
                Log.e(TAG, "setBandGain error: " + e);
            }
        }
    }

    public static void setGlobalVolume(float volDb) {
        globalVol = volDb;
    }

    public static void resetToFlat() {
        if (eq == null) return;
        for (int i = 0; i < NUM_BANDS; i++) {
            bandGains[i] = 0f;
            if (i < eq.getNumberOfBands()) {
                eq.setBandLevel((short)i, (short)0);
            }
        }
        globalVol = 0f;
    }

    public static float[] getFftData() {
        if (visualizer == null) return new float[0];
        byte[] fftBytes = visualizer.getFft();
        if (fftBytes == null || fftBytes.length < 2) return new float[0];
        int n = fftBytes.length / 2;
        float[] magnitudes = new float[n];
        for (int i = 1; i < n; i++) {
            int real = fftBytes[2 * i];
            int imag = fftBytes[2 * i + 1];
            magnitudes[i] = (float)Math.sqrt(real * real + imag * imag);
        }
        return magnitudes;
    }
}
"#;

// We assume the GlobalEq class is bundled and loadable.
static mut JAVA_GLOBALEQ_CLASS: Option<GlobalRef> = None;

fn install_java_source_and_init(env: &mut JNIEnv) -> Result<(), String> {
    let cls = env
        .find_class("com/example/minEq/GlobalEq")
        .map_err(|e| format!("find_class fail: {:?}", e))?;
    let global_cls = env
        .new_global_ref(cls)
        .map_err(|e| format!("new_global_ref fail: {:?}", e))?;
    unsafe {
        JAVA_GLOBALEQ_CLASS = Some(global_cls);
    }
    call_void_static_method(env, "initGlobalEq", "()V")?;
    Ok(())
}

fn call_void_static_method(env: &mut JNIEnv, method: &str, sig: &str) -> Result<(), String> {
    let cls = unsafe {
        JAVA_GLOBALEQ_CLASS
            .as_ref()
            .ok_or("GlobalEq class not stored")?
    };
    env.call_static_method(cls, method, sig, &[])
        .map_err(|e| format!("call_static_method fail: {:?}", e))?;
    Ok(())
}

fn call_set_band_gain(env: &mut JNIEnv, idx: jint, gain_db: jfloat) -> Result<(), String> {
    let cls = unsafe {
        JAVA_GLOBALEQ_CLASS
            .as_ref()
            .ok_or("GlobalEq class not stored")?
    };
    env.call_static_method(cls, "setBandGain", "(IF)V", &[idx.into(), gain_db.into()])
        .map_err(|e| format!("setBandGain call fail: {:?}", e))?;
    Ok(())
}

fn call_set_global_volume(env: &mut JNIEnv, vol_db: jfloat) -> Result<(), String> {
    let cls = unsafe {
        JAVA_GLOBALEQ_CLASS
            .as_ref()
            .ok_or("GlobalEq class not stored")?
    };
    env.call_static_method(cls, "setGlobalVolume", "(F)V", &[vol_db.into()])
        .map_err(|e| format!("setGlobalVolume call fail: {:?}", e))?;
    Ok(())
}

fn call_reset_to_flat(env: &mut JNIEnv) -> Result<(), String> {
    call_void_static_method(env, "resetToFlat", "()V")
}

fn call_get_fft_data(env: &mut JNIEnv) -> Result<Vec<f32>, String> {
    let cls = unsafe {
        JAVA_GLOBALEQ_CLASS
            .as_ref()
            .ok_or("GlobalEq class not stored")?
    };
    let ret = env
        .call_static_method(cls, "getFftData", "()[F", &[])
        .map_err(|e| format!("getFftData call fail: {:?}", e))?;
    let obj = ret
        .l()
        .map_err(|e| format!("getFftData ret.l() fail: {:?}", e))?;
    let arr = obj.into_raw() as jni::sys::jfloatArray;
    let len = env
        .get_array_length(arr)
        .map_err(|e| format!("get_array_length fail: {:?}", e))?;
    let mut buf = vec![0.0f32; len as usize];
    env.get_float_array_region(arr, 0, &mut buf)
        .map_err(|e| format!("get_float_array_region fail: {:?}", e))?;
    Ok(buf)
}

// Global pointer to the JNI environment pointer (set externally by Java)
static mut GLOBAL_JNI_ENV: Option<*mut jni::sys::JNIEnv> = None;

#[no_mangle]
pub extern "system" fn Java_com_example_minEq_MainActivity_storeJNIEnv(
    env: JNIEnv,
    _clazz: JClass,
) {
    let raw = env.get_native_interface();
    unsafe {
        GLOBAL_JNI_ENV = Some(raw);
    }
    println!("(storeJNIEnv) Stored global JNIEnv pointer");
}

fn with_jni_env<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut JNIEnv) -> Result<R, String>,
{
    unsafe {
        let ptr = GLOBAL_JNI_ENV.ok_or("No stored JNIEnv pointer")?;
        let mut env = JNIEnv::from_raw(ptr).map_err(|_| "JNIEnv from_raw failed")?;
        f(&mut env)
    }
}

// ---------- Android Entry Point ----------

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn android_main() {
    println!("===== EXTREME_EQ: android_main starting =====");
    dioxus_mobile::launch(app);
}

#[cfg(not(target_os = "android"))]
fn main() {
    println!("This app is intended for Android.");
}

// ---------- Dioxus App Components ----------

#[component]
fn app(cx: Scope) -> Element {
    let tailwind_data = "data:text/css;base64,Ly8gUHJvZHVjdGlvbiBUYWlsd2luZSBTV1MKYm9keSB7IGJhY2tncm91bmQtY29sb3I6ICMzMzM7IH0=";
    cx.render(rsx! {
        link { rel: "stylesheet", href: "{tailwind_data}" }
        div { class: "min-h-screen bg-gray-900 text-white flex flex-col items-center justify-center p-4",
            h1 { class: "text-2xl font-bold mb-4", "Global System EQ (Session 0)" }
            EqualizerUI {}
        }
    })
}

#[component]
fn EqualizerUI(cx: Scope) -> Element {
    let band_count = 8;
    let band_states: Vec<_> = (0..band_count).map(|_| use_state(cx, || 0.0_f32)).collect();
    let volume_state = use_state(cx, || 0.0_f32);

    use_effect(cx, || {
        let _ = with_jni_env(|env| call_void_static_method(env, "initGlobalEq", "()V"));
    });

    let spectro_data = use_state(cx, || vec![0.0_f32; 128]);
    use_effect(cx, || {
        let sp = spectro_data.clone();
        let running = Arc::new(AtomicBool::new(true));
        let run_clone = running.clone();
        let handle = thread::spawn(move || {
            while run_clone.load(Ordering::SeqCst) {
                if let Ok(data) = with_jni_env(|env| call_get_fft_data(env)) {
                    sp.set(data);
                }
                thread::sleep(Duration::from_millis(100));
            }
        });
        || {
            running.store(false, Ordering::SeqCst);
            let _ = handle.join();
        }
    });

    let set_band = move |idx: usize, db: f32| {
        band_states[idx].set(db);
        let _ = with_jni_env(|env| call_set_band_gain(env, idx as jint, db as jfloat));
    };
    let set_volume = {
        let volume_state = volume_state.clone();
        move |db: f32| {
            volume_state.set(db);
            let _ = with_jni_env(|env| call_set_global_volume(env, db as jfloat));
        }
    };
    let do_reset = move |_| {
        for st in &band_states {
            st.set(0.0);
        }
        volume_state.set(0.0);
        let _ = with_jni_env(|env| call_reset_to_flat(env));
    };

    cx.render(rsx! {
        div { class: "bg-gray-800 p-4 rounded-xl shadow-md w-full max-w-xl flex flex-col space-y-6",
            h2 { class: "text-xl font-bold", "Parametric EQ" }
            FrequencyChart { bands: band_states.iter().map(|s| *s.get()).collect(),
                             on_change: set_band }
            div { class: "flex flex-col items-center",
                label { class: "text-gray-300 font-semibold", "Global Volume" }
                input {
                    r#type: "range",
                    min: "-60",
                    max: "12",
                    step: "0.5",
                    value: "{*volume_state.get()}",
                    class: "w-full h-2 bg-gray-700 rounded-lg accent-pink-500 mt-1",
                    oninput: move |evt| {
                        if let Ok(v) = evt.value().parse::<f32>() {
                            set_volume(v);
                        }
                    }
                }
                span { class: "text-gray-400 text-sm mt-1", "{format!(\"{:.1} dB\", *volume_state.get())}" }
            }
            button { class: "bg-blue-700 text-white font-semibold py-2 rounded hover:bg-blue-600",
                     onclick: do_reset,
                     "Reset to Flat" }
            h3 { class: "text-lg font-semibold", "Spectrogram" }
            SpectrogramView { data: spectro_data.get().clone() }
        }
    })
}

// ---------- FrequencyChartProps and Component ----------

#[derive(Props, PartialEq)]
struct FrequencyChartProps {
    bands: Vec<f32>,
    on_change: fn(usize, f32),
}

#[component]
fn FrequencyChart(cx: Scope<FrequencyChartProps>) -> Element {
    let freq_centers = [60.0, 120.0, 250.0, 1000.0, 4000.0, 8000.0, 12000.0, 16000.0];
    let dragging = use_state(cx, || None::<usize>);
    let min_db = -24.0;
    let max_db = 24.0;
    let freq_to_x = |freq: f32| -> f32 {
        let f = freq.clamp(20.0, 20000.0);
        let t = (f.log10() - 20.0_f32.log10()) / (20000.0_f32.log10() - 20.0_f32.log10());
        t * 500.0
    };
    let db_to_y = |db: f32| -> f32 {
        let rel = (db.clamp(min_db, max_db) - min_db) / (max_db - min_db);
        200.0 - rel * 200.0
    };
    let points: Vec<(f32, f32)> = freq_centers
        .iter()
        .enumerate()
        .map(|(i, &f)| {
            let g = if i < cx.props.bands.len() {
                cx.props.bands[i]
            } else {
                0.0
            };
            (freq_to_x(f), db_to_y(g))
        })
        .collect();
    let line_str = points
        .iter()
        .map(|(x, y)| format!("{},{}", x, y))
        .collect::<Vec<_>>()
        .join(" ");
    let on_mouse_move = move |ev| {
        if let Some(idx) = *dragging.get() {
            let y = ev.page_coordinates().y as f32;
            let clamped_y = y.clamp(0.0, 200.0);
            let t = 1.0 - (clamped_y / 200.0);
            let db = min_db + t * (max_db - min_db);
            (cx.props.on_change)(idx, db);
        }
    };
    let on_mouse_up = move |_| {
        dragging.set(None);
    };
    cx.render(rsx! {
        div { class: "flex flex-col items-center",
            div { class: "text-gray-300 text-xs flex justify-between w-[500px]",
                { for (i, &f) in freq_centers.iter().enumerate() {
                    let label = if f < 1000.0 { format!("{:.0} Hz", f) } else { format!("{:.1} kHz", f / 1000.0) };
                    rsx!(span { key: "{i}", style: "width: 1px;", "{label}" })
                } }
            }
            svg { width: "500", height: "200", class: "bg-gray-700 mt-2 rounded",
                  onmousemove: on_mouse_move,
                  onmouseup: on_mouse_up,
                  polyline { fill: "none", stroke: "cyan", stroke_width: "2", points: "{line_str}" }
                  { for (i, (x, y)) in points.iter().enumerate() {
                      rsx!(circle { key: "{i}", cx: "{x}", cy: "{y}", r: "6", fill: "white", stroke: "cyan", stroke_width: "2",
                                     onmousedown: move |_| { dragging.set(Some(i)); },
                                     style: "cursor: grab;" })
                  } }
            }
            div { class: "flex justify-between w-[500px] text-gray-400 text-xs mt-1",
                  span { "-24 dB" }
                  span { "+24 dB" }
            }
        }
    })
}

// ---------- SpectrogramViewProps and Component ----------

#[derive(Props, PartialEq)]
struct SpectrogramViewProps {
    data: Vec<f32>,
}

#[component]
fn SpectrogramView(cx: Scope<SpectrogramViewProps>) -> Element {
    let w = 400.0;
    let h = 100.0;
    let bar_width = (w / (cx.props.data.len() as f32)).max(1.0_f32);
    cx.render(rsx! {
        div { class: "bg-gray-700 rounded w-[400px] h-[100px] flex flex-row items-end overflow-hidden",
              style: "position: relative;",
              { for (i, &val) in cx.props.data.iter().enumerate() {
                  let bar_h = val * h;
                  let hue = 240.0 - (val * 240.0);
                  let style_str = format!("width: {}px; height: {}px; background-color: hsl({},100%,50%);", bar_width, bar_h, hue);
                  rsx!(div { key: "{i}", style: "{style_str}" })
              } }
        }
    })
}
