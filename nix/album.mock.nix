# this file is the representation the default output of `munix manifest`

{ munix }:

munix.mkAlbum {

  name = "";

  origin.hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

  fetch.torrent = {
    file = ./Info/source.torrent;
    hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  };

  fetch.web = {
    url = "";
    hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  };

  cover = {
    file = ./cover.png;
    hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  };

  album = {
    metadata = {
    };
    mbid = {
    };
  };

  tracks = [
    # repeat for every file found by --tracks expression
    {
      file = "";
      metadata = {
      };
      mbid = {
      };
    }
  ];
}
