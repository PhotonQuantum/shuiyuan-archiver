(function() {
  if ('require' in window) {
    require("discourse/lib/theme-settings-store").registerSettings(27, {"immediate_reload":true,"show_section_header":false});
  }
})();
if ('define' in window) {
define("discourse/theme-27/initializers/theme-field-141-common-html-script-1", ["exports", "discourse/lib/plugin-api"], function (_exports, _pluginApi) {
  "use strict";

  Object.defineProperty(_exports, "__esModule", {
    value: true
  });
  _exports.default = void 0;

  var settings = require("discourse/lib/theme-settings-store").getObjectForTheme(27);

  var themePrefix = function themePrefix(key) {
    return "theme_translations.27.".concat(key);
  };

  var _default = {
    name: "theme-field-141-common-html-script-1",
    after: "inject-objects",
    initialize: function initialize() {
      (0, _pluginApi.withPluginApi)("0.8", function (api) {
        var h = require('virtual-dom').h;

        var ajax = require('discourse/lib/ajax').ajax;

        var themeSelector = require('discourse/lib/theme-selector');

        api.createWidget("theme-selector", {
          buildKey: function buildKey(attrs) {
            return "theme-selector";
          },
          defaultState: function defaultState() {
            return {
              currentThemeId: themeSelector.currentThemeId()
            };
          },
          click: function click(event) {
            var _this = this;

            var $target = $(event.target);
            var id = $target.data('id');
            var user = api.getCurrentUser();

            if (user) {
              user.findDetails().then(function (user) {
                var seq = user.get("user_option.theme_key_seq");

                _this.setTheme(id, seq);
              });
            } else {
              this.setTheme(id);
            }

            ;
            return true;
          },
          setTheme: function setTheme(themeId) {
            var seq = arguments.length > 1 && arguments[1] !== undefined ? arguments[1] : 0;

            if (themeId == null) {
              return;
            }

            themeSelector.setLocalTheme([themeId], seq);
            this.state.currentThemeId = themeId;

            if (settings.immediate_reload) {
              window.location.reload();
            } else {
              themeSelector.previewTheme([themeId]);
            }

            this.scheduleRerender();
          },
          themeHtml: function themeHtml(currentThemeId) {
            var themes = themeSelector.listThemes(this.site);

            if (themes && themes.length > 1) {
              return themes.map(function (theme) {
                var name = [theme.name];

                if (theme.id === currentThemeId) {
                  name.push('\xa0' + "*");
                }

                return h('li', {
                  attributes: {
                    "data-name": theme.name
                  }
                }, h('a.widget-link', {
                  attributes: {
                    "data-id": theme.id
                  }
                }, name));
              });
            }
          },
          html: function html(attrs, state) {
            var themeHtml = this.themeHtml(state.currentThemeId);
            var sectionHeader = null;
            var sectionHeaderText = I18n.t(themePrefix("hamburger_menu.theme_selector"));

            if (!themeHtml) {
              return;
            }

            if (settings.show_section_header) {
              var user = api.getCurrentUser();
              var sectionHeaderLink = null;

              if (user) {
                sectionHeaderLink = h('a.widget-link', {
                  href: "/my/preferences/interface"
                }, sectionHeaderText);
              } else {
                sectionHeaderLink = h('span', {}, sectionHeaderText);
              }

              sectionHeader = h('li', {
                style: "width: 100%;" + (user == null ? "padding: 0.25em 0.5em;" : null)
              }, sectionHeaderLink);
            }

            return [h('ul.menu-links.columned', [sectionHeader, themeHtml]), h('.clearfix'), h('hr')];
          }
        });
        api.decorateWidget('menu-links:before', function (helper) {
          if (helper.attrs.name === 'footer-links') {
            return [helper.widget.attach('theme-selector')];
          }
        });
      });
    }
  };
  _exports.default = _default;
});
}
