# Compile and install system-level GSettings schema database for isolated prefix tracking
custom_target(
  'compile-system-schemas',
  input: meson.project_source_root() / 'extension' / 'schemas' / 'org.gnome.shell.extensions.watchai.gschema.xml',
  output: 'gschemas.compiled',
  build_by_default: true,
  install: true,
  install_dir: datadir / 'glib-2.0' / 'schemas',
  install_mode: 'rw-r--r--',
  command: [
    glib_compile_schemas,
    '--targetdir=@OUTDIR@',
    meson.project_source_root() / 'extension' / 'schemas',
  ],
)
