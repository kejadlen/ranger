(function() {
    // === Backlog popover ===
    document.addEventListener('click', function(e) {
        var dialog = document.getElementById('backlog-dialog');
        if (dialog && dialog.open && !dialog.contains(e.target) && !e.target.closest('.backlog-trigger')) {
            dialog.close();
        }
    });

    // === Keyboard navigation ===
    function getFocusables() {
        return Array.from(document.querySelectorAll(
            'details.task > summary, div.task[data-key]'
        ));
    }
    function focusEl(els, idx) {
        if (idx >= 0 && idx < els.length) {
            els[idx].focus();
            els[idx].scrollIntoView({ block: 'nearest' });
        }
    }
    document.addEventListener('keydown', function(e) {
        if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;
        var els = getFocusables();
        var current = els.indexOf(document.activeElement);
        if (e.key === 'j' || e.key === 'ArrowDown') {
            e.preventDefault();
            focusEl(els, current < 0 ? 0 : current + 1);
        } else if (e.key === 'k' || e.key === 'ArrowUp') {
            e.preventDefault();
            focusEl(els, current < 0 ? 0 : current - 1);
        } else if ((e.key === 'Enter' || e.key === ' ') && document.activeElement.tagName === 'SUMMARY') {
            e.preventDefault();
            document.activeElement.click();
        }
    });

    // === Drag and drop (ready + icebox only) ===
    var DRAGGABLE_STATES = ['ready', 'icebox'];
    document.querySelectorAll('.drop-zone').forEach(function(z) {
        if (DRAGGABLE_STATES.indexOf(z.dataset.state) !== -1) {
            z.querySelectorAll('[data-key]').forEach(function(t) { t.draggable = true; });
        }
    });
    var draggedKey = null;
    var draggedEl = null;

    function getTaskEl(el) { return el.closest('[data-key]'); }
    function getDropZone(el) { return el.closest('.drop-zone'); }
    function isDraggableZone(zone) {
        return zone && DRAGGABLE_STATES.indexOf(zone.dataset.state) !== -1;
    }

    document.addEventListener('dragstart', function(e) {
        var task = getTaskEl(e.target);
        if (!task) return;
        var zone = getDropZone(task);
        if (!isDraggableZone(zone)) { e.preventDefault(); return; }
        draggedKey = task.dataset.key;
        draggedEl = task;
        task.classList.add('dragging');
        document.querySelectorAll('.drop-zone').forEach(function(z) {
            if (isDraggableZone(z)) z.classList.add('drag-active');
        });
        e.dataTransfer.effectAllowed = 'move';
        e.dataTransfer.setData('text/plain', draggedKey);
    });

    document.addEventListener('dragend', function() {
        if (draggedEl) draggedEl.classList.remove('dragging');
        document.querySelectorAll('.drop-indicator').forEach(function(el) { el.remove(); });
        document.querySelectorAll('.drop-zone-active').forEach(function(el) { el.classList.remove('drop-zone-active'); });
        document.querySelectorAll('.drag-active').forEach(function(el) { el.classList.remove('drag-active'); });
        draggedKey = null;
        draggedEl = null;
    });

    document.addEventListener('dragover', function(e) {
        var zone = getDropZone(e.target);
        if (!isDraggableZone(zone) || !draggedKey) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';

        document.querySelectorAll('.drop-indicator').forEach(function(el) { el.remove(); });
        document.querySelectorAll('.drop-zone-active').forEach(function(el) { el.classList.remove('drop-zone-active'); });

        var tasks = Array.from(zone.querySelectorAll('[data-key]'));
        if (tasks.length === 0) { zone.classList.add('drop-zone-active'); return; }

        var closestTask = null, insertBefore = true, minDist = Infinity;
        for (var i = 0; i < tasks.length; i++) {
            var rect = tasks[i].getBoundingClientRect();
            var midY = rect.top + rect.height / 2;
            var dist = Math.abs(e.clientY - midY);
            if (dist < minDist) { minDist = dist; closestTask = tasks[i]; insertBefore = e.clientY < midY; }
        }
        if (closestTask) {
            var indicator = document.createElement('div');
            indicator.className = 'drop-indicator';
            closestTask.parentNode.insertBefore(indicator, insertBefore ? closestTask : closestTask.nextSibling);
        }
    });

    document.addEventListener('drop', function(e) {
        e.preventDefault();
        var zone = getDropZone(e.target);
        if (!isDraggableZone(zone) || !draggedKey) return;

        var targetState = zone.dataset.state;
        var tasks = Array.from(zone.querySelectorAll('[data-key]'))
            .filter(function(t) { return t.dataset.key !== draggedKey; });

        var beforeKey = null, afterKey = null;
        if (tasks.length > 0) {
            var closestTask = null, insertBefore = true, minDist = Infinity;
            for (var i = 0; i < tasks.length; i++) {
                var rect = tasks[i].getBoundingClientRect();
                var midY = rect.top + rect.height / 2;
                var dist = Math.abs(e.clientY - midY);
                if (dist < minDist) { minDist = dist; closestTask = tasks[i]; insertBefore = e.clientY < midY; }
            }
            if (closestTask) {
                var idx = tasks.indexOf(closestTask);
                if (insertBefore) {
                    beforeKey = closestTask.dataset.key;
                    if (idx > 0) afterKey = tasks[idx - 1].dataset.key;
                } else {
                    afterKey = closestTask.dataset.key;
                    if (idx < tasks.length - 1) beforeKey = tasks[idx + 1].dataset.key;
                }
            }
        }

        var body = {};
        var draggedZone = draggedEl ? getDropZone(draggedEl) : null;
        var currentState = draggedZone ? draggedZone.dataset.state : null;
        if (targetState !== currentState) body.state = targetState;
        if (beforeKey) body.before = beforeKey;
        if (afterKey) body.after = afterKey;

        fetch('/api/tasks/' + encodeURIComponent(draggedKey) + '/move', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body)
        }).then(function(res) {
            if (res.ok) window.location.reload();
            else res.text().then(function(t) { console.error('Move failed:', t); });
        });
    });
})();
