# pulseaudio-events
Simple Rust client for PulseAudio that prints server-side events as they occur. Think `pactl subscribe` but with filtering and much improved performance for scripting etc.

# Usage
```
Listens for a specified event from PulseAudio and prints it.

USAGE:
    pulseaudio-events [FLAGS] [OPTIONS]

FLAGS:
    -d, --debug      Print all events, even ones not matching filters
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -f, --facility <facility>...      Facility to match against. Can be given more than once. Omit to allow all
                                      facilities. [possible values: sink, source, sink_input, source_output, module,
                                      client, sample_cache, server]
    -o, --operation <operation>...    Operation to match against. Can be given more than once. Omit to allow all
                                      operations. [possible values: new, changed, removed]
```
