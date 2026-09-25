class RequestSession {
  constructor() {
    this.generation = 0;
    this.pending = null;
  }

  begin() {
    if (this.pending !== null) return null;
    const request = ++this.generation;
    this.pending = request;
    return request;
  }

  isCurrent(request) {
    return this.pending === request;
  }

  finish(request) {
    if (!this.isCurrent(request)) return false;
    this.pending = null;
    return true;
  }

  get busy() {
    return this.pending !== null;
  }
}

if (typeof module !== 'undefined') module.exports = {RequestSession};

if (typeof document !== 'undefined') {
  (() => {
    const glyphs = {P: '♙', N: '♘', B: '♗', R: '♖', Q: '♕', K: '♔', p: '♟', n: '♞', b: '♝', r: '♜', q: '♛', k: '♚'};
    const board = document.querySelector('#board');
    const status = document.querySelector('#status');
    const error = document.querySelector('#error');
    const moves = document.querySelector('#move-list');
    const depth = document.querySelector('#depth');
    const depthValue = document.querySelector('#depth-value');
    const promotion = document.querySelector('#promotion');
    const newGame = document.querySelector('#new-game');
    const requests = new RequestSession();
    let state;
    let selected = null;
    let flipped = false;
    let pendingPromotion = null;

    const sayError = message => {
      error.textContent = message;
      error.hidden = !message;
    };
    const syncControls = () => {
      newGame.disabled = requests.busy;
    };
    async function api(path, method = 'GET', data) {
      const response = await fetch(path, {
        method,
        headers: data ? {'Content-Type': 'application/json'} : {},
        body: data ? JSON.stringify(data) : undefined,
      });
      let payload;
      try {
        payload = await response.json();
      } catch {
        throw new Error('The local server returned an invalid response.');
      }
      if (!response.ok) throw new Error(payload.error || 'The request could not be completed.');
      return payload;
    }
    function pieces(fen) {
      const result = {};
      fen.split(' ')[0].split('/').forEach((row, rowIndex) => {
        let file = 0;
        for (const char of row) {
          if (/\d/.test(char)) file += Number(char);
          else {
            result['abcdefgh'[file] + (8 - rowIndex)] = char;
            file++;
          }
        }
      });
      return result;
    }
    function render() {
      if (!state) return;
      const position = pieces(state.fen);
      const files = flipped ? 'hgfedcba' : 'abcdefgh';
      const ranks = flipped ? '12345678' : '87654321';
      const destinations = selected ? state.legalMoves.filter(move => move.slice(0, 2) === selected).map(move => move.slice(2, 4)) : [];
      board.replaceChildren();
      for (const rank of ranks) for (const file of files) {
        const square = file + rank;
        const piece = position[square];
        const cell = document.createElement('button');
        cell.className = `square ${(file.charCodeAt(0) - 97 + Number(rank)) % 2 ? 'dark' : 'light'}`;
        cell.type = 'button';
        cell.dataset.square = square;
        cell.setAttribute('role', 'gridcell');
        cell.setAttribute('aria-label', `${square}${piece ? ` ${piece}` : ''}`);
        if (square === selected) cell.classList.add('selected');
        if (destinations.includes(square)) cell.classList.add(position[square] ? 'capture' : 'target');
        cell.textContent = piece ? glyphs[piece] : '';
        if (file === files[0]) {
          const label = document.createElement('span');
          label.className = 'coordinate';
          label.textContent = rank;
          cell.append(label);
        }
        if (rank === ranks[ranks.length - 1]) {
          const label = document.createElement('span');
          label.className = 'coordinate file';
          label.textContent = file;
          cell.append(label);
        }
        cell.addEventListener('click', () => choose(square));
        board.append(cell);
      }
      status.textContent = state.status;
      document.querySelector('.panel').classList.toggle('thinking', state.thinking);
      depth.value = state.depth;
      depthValue.value = state.depth;
      moves.replaceChildren();
      for (let i = 0; i < state.moves.length; i += 2) {
        const item = document.createElement('li');
        item.textContent = `${state.moves[i]}${state.moves[i + 1] ? `  ${state.moves[i + 1]}` : ''}`;
        moves.append(item);
      }
      moves.scrollTop = moves.scrollHeight;
      syncControls();
    }
    function choose(square) {
      if (!state || requests.busy || state.terminal || state.thinking) return;
      const candidates = state.legalMoves.filter(move => move.slice(0, 2) === selected && move.slice(2, 4) === square);
      if (candidates.length) {
        if (candidates.some(move => move.length === 5)) {
          pendingPromotion = candidates;
          promotion.returnValue = '';
          promotion.showModal();
        } else {
          play(candidates[0]);
        }
        return;
      }
      selected = state.legalMoves.some(move => move.slice(0, 2) === square) ? square : null;
      render();
    }
    async function play(move) {
      if (!state || requests.busy) return;
      const request = requests.begin();
      if (request === null) return;
      selected = null;
      sayError('');
      state.thinking = true;
      render();
      try {
        const next = await api('/api/move', 'POST', {move});
        if (!requests.isCurrent(request)) return;
        state = next;
      } catch (err) {
        if (!requests.isCurrent(request)) return;
        sayError(err.message);
        state.thinking = false;
      } finally {
        if (requests.finish(request)) render();
      }
    }
    async function startNewGame() {
      if (requests.busy) return;
      const request = requests.begin();
      if (request === null) return;
      syncControls();
      selected = null;
      pendingPromotion = null;
      if (promotion.open) promotion.close();
      sayError('');
      render();
      try {
        const next = await api('/api/new', 'POST', {depth: Number(depth.value)});
        if (!requests.isCurrent(request)) return;
        state = next;
      } catch (err) {
        if (!requests.isCurrent(request)) return;
        sayError(err.message);
      } finally {
        if (requests.finish(request)) render();
      }
    }
    async function loadState() {
      const request = requests.begin();
      if (request === null) return;
      syncControls();
      try {
        const next = await api('/api/state');
        if (!requests.isCurrent(request)) return;
        state = next;
      } catch (err) {
        if (!requests.isCurrent(request)) return;
        status.textContent = 'Unable to load board';
        sayError(err.message);
      } finally {
        if (requests.finish(request)) {
          render();
          syncControls();
        }
      }
    }

    promotion.addEventListener('cancel', () => {
      pendingPromotion = null;
    });
    promotion.addEventListener('close', () => {
      const choice = promotion.returnValue;
      const candidates = pendingPromotion;
      pendingPromotion = null;
      if (choice && candidates) {
        const move = candidates.find(candidate => candidate.endsWith(choice));
        if (move) play(move);
      }
    });
    newGame.addEventListener('click', startNewGame);
    document.querySelector('#flip').addEventListener('click', () => {
      flipped = !flipped;
      render();
    });
    depth.addEventListener('input', () => {
      depthValue.value = depth.value;
    });
    loadState();
  })();
}
