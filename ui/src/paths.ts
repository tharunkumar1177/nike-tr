/** The folder that contains `path`, e.g. `C:\work` for `C:\work\amp`. */
export function parentDirectory(path: string): string {
  const index = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  if (index <= 0) {
    return "";
  }
  const parent = path.slice(0, index);
  return parent.endsWith(":") ? `${parent}\\` : parent;
}

/** Joins a folder and a name with the separator the folder already uses. */
export function joinPath(directory: string, name: string): string {
  if (directory.length === 0) {
    return name;
  }
  if (directory.endsWith("\\") || directory.endsWith("/")) {
    return directory + name;
  }
  const separator =
    directory.includes("/") && !directory.includes("\\") ? "/" : "\\";
  return directory + separator + name;
}
