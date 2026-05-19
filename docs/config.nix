# this file is a documentation of ~/.config/mue/config.nix

{
  # path to custom store
  # it will contain all sources + built albums + entire build system env
  store = "";

  # path to staging directory all `fetch` and first-time `build` will run against
  # it will contain fetched source data in ./{source_type}/{sanitized_source_name}-{nix32_hash} format
  origin = "";

  # tips:
  #   - point both to large storage disk
  #   - backup store periodically; it is the actual source of your entire library
  #   - to garbage collect old albums not in current use run `nix store gc --store ${store}`

  # determines existence of additional libaries based on extention
  library = {

    # FLAC library
    # album counts towards one if all input tracks have .flac extension
    flac = {

      # will flac albums be built at all
      enable = true;

      # path to directory where flac album folders will be created
      # naming pattern "AlbumArtist - Album" is used
      # each album folder is populated purely by symlinks to custom store by rust
      root = "";

      # will flac album contents be linked by rust to folder containing album.nix
      link_to_album_root = true;

      # will flac album contents be linked by rust to folder in library.flac.root
      link_to_library_root = true;
    };

    # OPUS library
    opus = {

      # will albums be built to have .opus clones
      enable = true;

      # used as argument for conversion
      # optional: defaults to 128 if missing
      kbps = 128;

      # path to directory where opus album clone folders will be created
      root = "";

      # will opus album contents be linked by rust to folder containing album.nix
      link_to_album_root = false;

      # will opus album contents be linked by rust to folder in library.opus.root
      link_to_library_root = true;
    };
  };

  # commands to run on `fetch` and `build`
  # ${origin.path} resolution happens automatically based on --source specified
  commands = {

    # --source torrent
    torrent = {

      # runs at `mue fetch`
      # reads the torrent file and starts download to origin
      # recommended command:
      # fetch = "transmission-remote -a '${source.torrent.file}' -w '${origin.path}'";
      fetch = "";

      # runs at `mue build`
      # used to verify 100% seedability and pairity to ${source.torrent.file}
      # skipped if auto-resolved ${origin.path} is already in custom store
      # recommended command:
      # verify = "imdl torrent verify '${source.torrent.file}' --content '${origin.path}/${source.torrent.name}'";
      verify = "";

      # runs after successful `mue build`
      # used to ping the torrent daemon with custom-store-bound ${origin.path} to seed from it directly
      seed = "transmission-remote -a '${source.torrent.file}' -w '${origin.path}'";
    };

    # --source torrent
    web = {

      # runs at `mue fetch`
      # recommended command:
      # fetch = "curl -L '${source.web.url}' -o '${origin.path}'";
      fetch = "";
    };
  };
}
