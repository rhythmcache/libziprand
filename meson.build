project('ziprand', 'c',
  version: '1.0.0',
  license: 'Apache-2.0',
  default_options: [
    'c_std=c17',
    'warning_level=2',
  ]
)

version_arr = meson.project_version().split('.')
major_version = version_arr[0].to_int()
minor_version = version_arr[1].to_int()
patch_version = version_arr[2].to_int()

soversion = '0'

prefix = get_option('prefix')
includedir = get_option('includedir')
libdir = get_option('libdir')

sources = files(
  'ziprand.c',
  'ziprand_helpers.c'
)
headers = files('ziprand.h')

# Shared library
libziprand = library(
  'ziprand',
  sources,
  version: meson.project_version(),
  soversion: soversion,
  install: true,
  include_directories: include_directories('.'),
)

# Static library (different internal target name)
libziprand_static = static_library(
  'ziprand_static',
  sources,
  install: true,
  include_directories: include_directories('.'),
)

# Install headers
install_headers(headers)

# pkg-config file
pkg = import('pkgconfig')
pkg.generate(
  libraries: libziprand,
  name: 'libziprand',
  description: 'A lightweight C library for random-access operations on ZIP archives.',
  version: meson.project_version(),
  filebase: 'libziprand',
)

# Dependency object for Meson subprojects
ziprand_dep = declare_dependency(
  link_with: libziprand,
  include_directories: include_directories('.'),
)
