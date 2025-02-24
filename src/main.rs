use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        link { rel: "stylesheet", href: "/assets/tailwind.css" }
        div {
            class: "min-h-screen bg-gray-900 flex flex-col items-center justify-center p-4",
            Equalizer {}
        }
    }
}

#[component]
fn Equalizer() -> Element {
    // State for equalizer bands (in dB, -12 to +12)
    let mut bass = use_signal(|| 0);
    let mut mid = use_signal(|| 0);
    let mut treble = use_signal(|| 0);

    rsx! {
        div {
            class: "bg-gray-800 p-6 rounded-xl shadow-lg w-full max-w-md",

            // Title
            h1 {
                class: "text-white text-2xl font-bold mb-6 text-center",
                "Audio Equalizer"
            }

            // Frequency Bands
            div {
                class: "space-y-6",

                // Bass Slider
                div {
                    label {
                        class: "text-gray-300 block mb-2",
                        "Bass (60 Hz)"
                    }
                    input {
                        r#type: "range",
                        min: "-12",
                        max: "12",
                        value: "{bass}",
                        class: "w-full h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer accent-blue-500",
                        oninput: move |event| {
                            if let Ok(value) = event.value().parse::<i32>() {
                                bass.set(value);
                            }
                        }
                    }
                    span {
                        class: "text-gray-400 text-sm text-center block mt-1",
                        "{bass} dB"
                    }
                }

                // Mid Slider
                div {
                    label {
                        class: "text-gray-300 block mb-2",
                        "Mid (1 kHz)"
                    }
                    input {
                        r#type: "range",
                        min: "-12",
                        max: "12",
                        value: "{mid}",
                        class: "w-full h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer accent-blue-500",
                        oninput: move |event| {
                            if let Ok(value) = event.value().parse::<i32>() {
                                mid.set(value);
                            }
                        }
                    }
                    span {
                        class: "text-gray-400 text-sm text-center block mt-1",
                        "{mid} dB"
                    }
                }

                // Treble Slider
                div {
                    label {
                        class: "text-gray-300 block mb-2",
                        "Treble (8 kHz)"
                    }
                    input {
                        r#type: "range",
                        min: "-12",
                        max: "12",
                        value: "{treble}",
                        class: "w-full h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer accent-blue-500",
                        oninput: move |event| {
                            if let Ok(value) = event.value().parse::<i32>() {
                                treble.set(value);
                            }
                        }
                    }
                    span {
                        class: "text-gray-400 text-sm text-center block mt-1",
                        "{treble} dB"
                    }
                }
            }

            // Reset Button
            button {
                class: "mt-6 w-full bg-blue-500 hover:bg-blue-600 text-white font-semibold py-2 rounded-lg transition-colors",
                onclick: move |_| {
                    bass.set(0);
                    mid.set(0);
                    treble.set(0);
                },
                "Reset to Flat"
            }
        }
    }
}
