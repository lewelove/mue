# this file is the documentation of film.nix attributes

{ mute }:

mute.mkFilm {

  # film name in "{title}-{year}-{director} -> lowercase -> sanitize with `-`" format
  name = "";

  # origin is required to separate the imperative fetch from actual local hash pinned data the album is built from
  origin = {
    # auto resolved by rust within staging directory based on selected source or lack there of
    path = "";
    # NAR hash of origin.path
    hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  };

  source = {
    torrent = {
      # path to .torrent file
      file = ./Info/source.torrent;
      # name used to specify source in staging directory
      # falls back to auto resolution by rust based on Torrent Name of fetch.torrent.file
      name = ;
      # file hash of fetch.torrent.file
      hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    };
    web = {
      # url to fetch .zip or folder from
      url = "";
      # NAR hash of files fetched by source.web.url 
      hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    };
  };

  poster = {
    file = ./poster.png;
    # file hash of poster.file
    hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  };

  film = {
    # film's metadata
    metadata = {
      title = "";
      year = "";
      director = "";
    };
    urls = {
      # url to https://letterboxd.com/film/...
      letterboxd = "";
    };
    # path to video file
    video = "";
  };
}
