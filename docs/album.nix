# this file is the documentation of album.nix attributes

{ munix }:

munix.mkAlbum {

  # album name in "{albumartist}-{album} -> lowercase -> sanitize with `-`" format
  name = "";

  # origin is required to separate the imperative fetch from actual local hash pinned data the album is built from
  origin = {
    # auto resolved by rust within staging directory based on selected source or lack there of
    path = "";
    # NAR hash of origin.path
    hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  };

  fetch = {
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

  cover = {
    file = ./cover.png;
    # file hash of cover.file
    hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  };

  album = {
    # metadata keys for all tracks following Vorbis standard
    metadata = {
    };
    # list of musicbrainz ids for all tracks 
    mbid = {
      musicbrainz_albumid = "";
      musicbrainz_albumartistid = "";
      musicbrainz_releasegroupid = "";
    };
  };

  tracks = [
    {
      # path to track file relative to origin.path directory
      file = "";
      # metadata keys for specific track following Vorbis standard
      metadata = {
      };
      # list of musicbrainz ids for specific track
      mbid = {
        musicbrainz_trackid = "";
        musicbrainz_releasetrackid = "";
        musicbrainz_artistid = "";
      };
    }
    {
      etc...
    }
  ];
}
