hljs.highlightAll();

function selectLang(lang) {
    document.querySelectorAll('.tabs').forEach(tabBar => {
        const group = tabBar.dataset.group;
        const tab = tabBar.querySelector(`.tab[data-lang="${lang}"]`);
        if (!tab) return;
        tabBar.querySelectorAll('.tab').forEach(
            t => t.classList.remove('active')
        );
        document.querySelectorAll(
            `.panel[data-group="${group}"]`
        ).forEach(p => p.classList.remove('active'));
        tab.classList.add('active');
        const panel = document.getElementById(group + '-' + lang);
        if (panel) panel.classList.add('active');
    });
}

document.querySelectorAll('.tabs .tab').forEach(tab => {
    tab.addEventListener('click', () => selectLang(tab.dataset.lang));
});
