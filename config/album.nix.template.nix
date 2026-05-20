{ data, lib }:
let
  toNixVal = v:
    if builtins.isString v then (if v == "" then "\"\"" else ''"${v}"'')
    else if builtins.isInt v then toString v
    else if builtins.isBool v then (if v then "true" else "false")
    else if builtins.isList v then "[ " + builtins.concatStringsSep " " (builtins.map toNixVal v) + " ]"
    else "\"\"";

  get = path: default:
    let
      parts = lib.splitString "." path;
      step = acc: key: if builtins.isAttrs acc && acc ? ${key} then acc.${key} else null;
      res = builtins.foldl' step data parts;
    in if res == null then default else res;

  totalDiscs = get "album.info.total_discs" 1;

  renderSource = if data.source.type == "torrent" then
    "  source.torrent = {\n" +
    "    file = ./${get "source.torrent.file" "source.torrent"};\n" +
    "    name = ${toNixVal (get "source.torrent.name" "")};\n" +
    "    hash = \"${get "source.torrent.hash" "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}\";\n" +
    "  };"
  else
    "  source.web = {\n" +
    "    url = \"${get "source.web.url" ""}\";\n" +
    "    hash = \"${get "source.web.hash" "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}\";\n" +
    "  };";

  renderAlbum = 
    "  album = {\n" +
    "    metadata = {\n" +
    "      albumartist = ${toNixVal (get "album.metadata.albumartist" "")};\n" +
    "      album = ${toNixVal (get "album.metadata.album" "")};\n" +
    "      date = ${toNixVal (get "album.metadata.date" "")};\n" +
    "\n" +
    "      genre = ${toNixVal (get "album.metadata.genre" [])};\n" +
    "      styles = ${toNixVal (get "album.metadata.styles" [])};\n" +
    "\n" +
    "      original_date = ${toNixVal (get "album.metadata.original_date" "")};\n" +
    "\n" +
    "      country = ${toNixVal (get "album.metadata.country" "")};\n" +
    "      label = ${toNixVal (get "album.metadata.label" "")};\n" +
    "      catalognumber = ${toNixVal (get "album.metadata.catalognumber" "")};\n" +
    "      release_date = ${toNixVal (get "album.metadata.release_date" "")};\n" +
    "    };\n" +
    "    url = {\n" +
    "      ctdbtocid = ${toNixVal (get "album.url.ctdbtocid" "")};\n" +
    "      discogs_release = ${toNixVal (get "album.url.discogs_release" "")};\n" +
    "      musicbrainz_release = ${toNixVal (get "album.url.musicbrainz_release" "")};\n" +
    "      discogs_master = ${toNixVal (get "album.url.discogs_master" "")};\n" +
    "      musicbrainz_release_group = ${toNixVal (get "album.url.musicbrainz_release_group" "")};\n" +
    "    };\n" +
    "    mbid = {\n" +
    "      musicbrainz_albumid = ${toNixVal (get "album.mbid.musicbrainz_albumid" "")};\n" +
    "      musicbrainz_albumartistid = ${toNixVal (get "album.mbid.musicbrainz_albumartistid" "")};\n" +
    "      musicbrainz_releasegroupid = ${toNixVal (get "album.mbid.musicbrainz_releasegroupid" "")};\n" +
    "    };\n" +
    "  };";

  renderTrack = t:
    "    {\n" +
    "      file = \"${t.file or ""}\";\n" +
    "      metadata = {\n" +
    (if totalDiscs > 1 then "        discnumber = ${toNixVal (t.metadata.discnumber or 1)};\n" else "") +
    "        tracknumber = ${toNixVal (t.metadata.tracknumber or 0)};\n" +
    "        title = ${toNixVal (t.metadata.title or "Untitled")};\n" +
    "        artist = ${toNixVal (t.metadata.artist or "")};\n" +
    "      };\n" +
    "      mbid = {\n" +
    "        musicbrainz_trackid = ${toNixVal (t.mbid.musicbrainz_trackid or "")};\n" +
    "        musicbrainz_releasetrackid = ${toNixVal (t.mbid.musicbrainz_releasetrackid or "")};\n" +
    "        musicbrainz_artistid = ${toNixVal (t.mbid.musicbrainz_artistid or "")};\n" +
    "      };\n" +
    "    }";

in
''
{ mue }:

mue.mkAlbum {

  name = "${data.name or ""}";

  origin.hash = "${get "origin.hash" "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}";

${renderSource}

  cover = {
    file = ./${get "cover.file" "cover.png"};
    hash = "${get "cover.hash" "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}";
  };

${renderAlbum}

  tracks = [
${builtins.concatStringsSep "\n" (builtins.map renderTrack (data.tracks or []))}
  ];
}''
