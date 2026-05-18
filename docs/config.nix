# this file is a documentation of ~/.config/munix/config.nix

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
  #   - to garbage collect old albums not in current use run `nix store gc --store {store}`

  # commands to run on `fetch` and `build`
  # ${origin.path} resolution happens automatically based on --source specified
  commands = {

    # --source torrent
    torrent = {

      # runs at `munix fetch`
      # reads the torrent file and starts download to origin
      # recommended command:
      # fetch = "transmission-remote -a '${source.torrent.file}' -w '${origin.path}'";
      fetch = "";

      # runs at `munix build`
      # used to verify 100% seedability and pairity to ${source.torrent.file}
      # skipped if auto-resolved ${origin.path} is already in custom store
      # recommended command:
      # verify = "imdl torrent verify '${source.torrent.file}' --content '${origin.path}/${source.torrent.name}'";
      verify = "";

      # runs after successful `munix build`
      # used to ping the torrent daemon with custom-store-bound ${origin.path} to seed from it directly
      seed = "transmission-remote -a '${source.torrent.file}' -w '${origin.path}'";
    };

    # --source torrent
    web = {

      # runs at `munix fetch`
      # recommended command:
      # fetch = "curl -L '${source.web.url}' -o '${origin.path}'";
      fetch = "";
    };
  };
}

