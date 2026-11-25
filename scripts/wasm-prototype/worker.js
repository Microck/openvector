const engine = createTracer();
engine.then(() => postMessage({ready: true}));
onmessage = async ({data}) => {
  try {
    const module = await engine;
    module.FS.writeFile('input.png', new Uint8Array(data));
    try {
      const ms = module._convert_png();
      if (ms < 0) throw new Error(module.FS.readFile('error.txt', {encoding: 'utf8'}));
      const svg = module.FS.readFile('output.svg');
      postMessage({ms, svg}, [svg.buffer]);
    } finally {
      for (const name of ['input.png', 'output.svg', 'error.txt']) {
        if (module.FS.analyzePath(name).exists) module.FS.unlink(name);
      }
    }
  } catch (error) {
    postMessage({error: String(error)});
  }
};
