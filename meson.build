project(
  'watchai',
  version: '0.1.0',
  meson_version: '>=0.60.0',
)

gnome = import('gnome')

python3 = find_program('python3', required: true)
cargo = find_program('cargo', required: true)
glib_compile_schemas = find_program('glib-compile-schemas', required: true)

prefix = get_option('prefix')
bindir = get_option('bindir')
datadir = get_option('datadir')

full_bindir = prefix / bindir
full_datadir = prefix / datadir

extension_uuid = 'watchai@gnome.org'
extension_dir = datadir / 'gnome-shell' / 'extensions' / extension_uuid

# 1. Compile production daemon binary using isolated Cargo target
cargo_build_script = meson.project_source_root() / 'build-aux' / 'cargo-build.py'

daemon_binary = custom_target(
  'watchai-daemon',
  output: 'watchai-daemon',
  build_by_default: true,
  install: true,
  install_dir: bindir,
  install_mode: 'rwxr-xr-x',
  command: [
    python3,
    cargo_build_script,
    '--source-dir', meson.project_source_root(),
    '--build-dir', meson.current_build_dir(),
    '--output', '@OUTPUT@',
    '--package', 'watchai-daemon',
    '--release',
  ],
)

# 2. Systemd user service unit
conf_data = configuration_data()
conf_data.set('bindir', full_bindir)

configure_file(
  input: 'systemd/watchai.service.in',
  output: 'watchai.service',
  configuration: conf_data,
  install: true,
  install_dir: datadir / 'systemd' / 'user',
  install_mode: 'rw-r--r--',
)

# 3. GNOME Shell extension files (test files strictly excluded)
extension_files = [
  'extension/dbus_client.js',
  'extension/extension.js',
  'extension/indicator.js',
  'extension/metadata.json',
  'extension/notifications.js',
  'extension/popover.js',
  'extension/settings.js',
  'extension/stylesheet.css',
  'extension/utils.js',
]

install_data(
  extension_files,
  install_dir: extension_dir,
  install_mode: 'rw-r--r--',
)

# 4. GSettings schemas: dual-location strategy
# Local extension schema directory for direct user-local resolution in GNOME Shell
install_data(
  'extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml',
  install_dir: extension_dir / 'schemas',
  install_mode: 'rw-r--r--',
)

custom_target(
  'compile-extension-schemas',
  input: 'extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml',
  output: 'gschemas.compiled',
  build_by_default: true,
  install: true,
  install_dir: extension_dir / 'schemas',
  install_mode: 'rw-r--r--',
  command: [
    glib_compile_schemas,
    '--targetdir=@OUTDIR@',
    meson.project_source_root() / 'extension' / 'schemas',
  ],
)

# System-wide GLib schema directory
install_data(
  'extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml',
  install_dir: datadir / 'glib-2.0' / 'schemas',
  install_mode: 'rw-r--r--',
)

gnome.post_install(
  glib_compile_schemas: true,
)
