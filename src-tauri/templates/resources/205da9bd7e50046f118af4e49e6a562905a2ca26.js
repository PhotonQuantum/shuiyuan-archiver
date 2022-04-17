(function() {
  if ('require' in window) {
    require("discourse/lib/theme-settings-store").registerSettings(18, {"minimum_trust_level_to_create_TOC":0,"composer_toc_text":"此主题将会生成目录","table_of_contents_icon":"align-left","anchor_icon":"hashtag","theme_uploads":{"icons-sprite":"/uploads/default/original/3X/8/0/80ed408554201b1aea5b03b7b3a2ab0b0be0a012.svg"}});
  }
})();
if ('define' in window) {
define("discourse/theme-18/initializers/theme-field-64-common-html-script-1", ["exports", "discourse/lib/plugin-api"], function (_exports, _pluginApi) {
  "use strict";

  Object.defineProperty(_exports, "__esModule", {
    value: true
  });
  _exports.default = void 0;

  var settings = require("discourse/lib/theme-settings-store").getObjectForTheme(18);

  var themePrefix = function themePrefix(key) {
    return "theme_translations.18.".concat(key);
  };

  var _default = {
    name: "theme-field-64-common-html-script-1",
    after: "inject-objects",
    initialize: function initialize() {
      var _this = this;

      (0, _pluginApi.withPluginApi)("0.1", function (api) {
        var minimumOffset = require("discourse/lib/offset-calculator").minimumOffset;

        var _require = require("discourse-common/lib/icon-library"),
            iconHTML = _require.iconHTML;

        var _Ember = Ember,
            run = _Ember.run;
        var mobileView = $("html").hasClass("mobile-view");
        var linkIcon = iconHTML(settings.anchor_icon);
        var closeIcon = iconHTML("times");
        var dtocIcon = iconHTML("align-left");
        var currUser = api.getCurrentUser();
        var currUserTrustLevel = currUser ? currUser.trust_level : "";
        var minimumTrustLevel = settings.minimum_trust_level_to_create_TOC;
        var SCROLL_THROTTLE = 300;
        var SMOOTH_SCROLL_SPEED = 300;
        var TOC_ANIMATION_SPEED = 300;

        var cleanUp = function cleanUp(item) {
          var cleanItem = item.trim().toLowerCase().replace(/[\{\}\[\]\\\/\<\>\(\)\|\+\?\*\^\'\`\'\"\.\_\$\s~!@#%&,;:=]/gi, "-").replace(/\-\-+/g, "-").replace(/^\-/, "").replace(/\-$/, "");
          return cleanItem;
        };

        var setUpTocItem = function setUpTocItem(item) {
          var unique = item.attr("id");
          var text = item.text();
          var tocItem = $("<li/>", {
            class: "d-toc-item",
            "data-d-toc": unique
          });
          tocItem.append($("<a/>", {
            text: text
          }));
          return tocItem;
        };

        (function (dToc) {
          var _arguments = arguments,
              _this3 = this;

          dToc($, window);
          $.widget("discourse.dToc", {
            _create: function _create() {
              this.generateDtoc();
              this.setEventHandlers();
            },
            generateDtoc: function generateDtoc() {
              var self = this;
              var primaryHeadings = $(this.options.cooked).find(this.options.selectors.substr(0, this.options.selectors.indexOf(",")));
              self.element.addClass("d-toc");
              primaryHeadings.each(function (index) {
                var selectors = self.options.selectors,
                    ul = $("<ul/>", {
                  id: "d-toc-top-heading-".concat(index),
                  class: "d-toc-heading"
                });
                ul.append(setUpTocItem($(this)));
                self.element.append(ul);
                $(this).nextUntil(this.nodeName.toLowerCase()).each(function () {
                  var headings = $(this).find(selectors).length ? $(this).find(selectors) : $(this).filter(selectors);
                  headings.each(function () {
                    self.nestTocItem.call(this, self, ul);
                  });
                });
              });
            },
            nestTocItem: function nestTocItem(self, ul) {
              var index = $(this).index(self.options.selectors);
              var previousHeader = $(self.options.selectors).eq(index - 1);
              var previousTagName = previousHeader.prop("tagName").charAt(1);
              var currentTagName = $(this).prop("tagName").charAt(1);

              if (currentTagName < previousTagName) {
                self.element.find(".d-toc-subheading[data-tag=\"".concat(currentTagName, "\"]")).last().append(setUpTocItem($(this)));
              } else if (currentTagName === previousTagName) {
                ul.find(".d-toc-item").last().after(setUpTocItem($(this)));
              } else {
                ul.find(".d-toc-item").last().after($("<ul/>", {
                  class: "d-toc-subheading",
                  "data-tag": currentTagName
                })).next(".d-toc-subheading").append(setUpTocItem($(this)));
              }
            },
            setEventHandlers: function setEventHandlers() {
              var _this2 = this;

              var self = this;

              var dtocMobile = function dtocMobile() {
                $(".d-toc").toggleClass("d-toc-mobile");
              };

              this.element.on("click.d-toc", "li", function () {
                self.element.find(".d-toc-active").removeClass("d-toc-active");
                $(this).addClass("d-toc-active");

                if (mobileView) {
                  dtocMobile();
                } else {
                  var elem = $("li[data-d-toc=\"".concat($(this).attr("data-d-toc"), "\"]"));
                  self.triggerShowHide(elem);
                }

                self.scrollTo($(this));
              });
              $("#main").on("click.toggleDtoc", ".d-toc-toggle, .d-toc-close, .post-bottom-wrapper a", dtocMobile);

              var onScroll = function onScroll() {
                run.throttle(_this2, self.highlightItemsOnScroll, self, SCROLL_THROTTLE);
              };

              $(window).on("scroll.d-toc", onScroll);
            },
            highlightItemsOnScroll: function highlightItemsOnScroll(self) {
              $("html, body").promise().done(function () {
                var winScrollTop = $(window).scrollTop();
                var anchors = $(self.options.cooked).find("[data-d-toc]");
                var closestAnchorDistance = null;
                var closestAnchorIdx = null;
                anchors.each(function (idx) {
                  var distance = Math.abs($(this).offset().top - minimumOffset() - winScrollTop);

                  if (closestAnchorDistance == null || distance < closestAnchorDistance) {
                    closestAnchorDistance = distance;
                    closestAnchorIdx = idx;
                  } else {
                    return false;
                  }
                });
                var anchorText = $(anchors[closestAnchorIdx]).attr("data-d-toc");
                var elem = $("li[data-d-toc=\"".concat(anchorText, "\"]"));

                if (elem.length) {
                  self.element.find(".d-toc-active").removeClass("d-toc-active");
                  elem.addClass("d-toc-active");
                }

                if (!mobileView) {
                  self.triggerShowHide(elem);
                }
              });
            },
            triggerShowHide: function triggerShowHide(elem) {
              if (elem.parent().is(".d-toc-heading") || elem.next().is(".d-toc-subheading")) {
                this.showHide(elem.next(".d-toc-subheading"));
              } else if (elem.parent().is(".d-toc-subheading")) {
                this.showHide(elem.parent());
              }
            },
            showHide: function showHide(elem) {
              return elem.is(":visible") ? this.hide(elem) : this.show(elem);
            },
            hide: function hide(elem) {
              var target = $(".d-toc-subheading").not(elem).not(elem.parents(".d-toc-subheading:has(.d-toc-active)"));
              return target.slideUp(TOC_ANIMATION_SPEED);
            },
            show: function show(elem) {
              return elem.slideDown(TOC_ANIMATION_SPEED);
            },
            scrollTo: function scrollTo(elem) {
              var currentDiv = $("[data-d-toc=\"".concat(elem.attr("data-d-toc"), "\"]"));
              $("html, body").animate({
                scrollTop: "".concat(currentDiv.offset().top - minimumOffset())
              }, {
                duration: SMOOTH_SCROLL_SPEED
              });
            },
            setOptions: function setOptions() {
              $.Widget.prototype._setOptions.apply(_this3, _arguments);
            }
          });
        })(function () {});

        api.decorateCooked(function ($elem) {
          run.scheduleOnce("actions", function () {
            if ($elem.hasClass("d-editor-preview")) return;
            if (!$elem.parents("article#post_1").length) return;
            var dToc = $elem.find("[data-theme-toc=\"true\"]");
            if (!dToc.length) return _this;
            var body = $elem;
            body.find("div, aside, blockquote, article, details").each(function () {
              $(this).children("h1,h2,h3,h4,h5,h6").each(function () {
                $(this).replaceWith("<div class=\"d-toc-ignore\">".concat($(this).html(), "</div>"));
              });
            });
            body.append("<span id=\"bottom-anchor\" class=\"d-toc-igonore\"></span>");
            var dTocHeadingSelectors = "h1,h2,h3,h4,h5,h6";

            if (!body.has(">h1").length) {
              dTocHeadingSelectors = "h2,h3,h4,h5,h6";

              if (!body.has(">h2").length) {
                dTocHeadingSelectors = "h3,h4,h5,h6";

                if (!body.has(">h3").length) {
                  dTocHeadingSelectors = "h4,h5,h6";

                  if (!body.has(">h4").length) {
                    dTocHeadingSelectors = "h5,h6";

                    if (!body.has(">h5").length) {
                      dTocHeadingSelectors = "h6";
                    }
                  }
                }
              }
            }

            body.find(dTocHeadingSelectors).each(function () {
              if ($(this).hasClass("d-toc-ignore")) return;
              var heading = $(this);
              var id = heading.attr("id") || "";

              if (!id.length) {
                id = cleanUp(heading.text());
              }

              heading.attr({
                id: id,
                "data-d-toc": id
              }).addClass("d-toc-post-heading");
            });
            body.addClass("d-toc-cooked").prepend("<span class=\"d-toc-toggle\">\n                      ".concat(dtocIcon, " ").concat(I18n.t(themePrefix("table_of_contents")), "\n                      </span>")).parents(".regular").addClass("d-toc-regular").parents("article").addClass("d-toc-article").append("<div class=\"d-toc-main\">\n                  <div class=\"post-bottom-wrapper dekstop\">\n                    <a href=\"#bottom-anchor\" title=\"".concat(I18n.t(themePrefix("post_bottom_tooltip")), "\">").concat(iconHTML("downward"), "</a>\n                    </div>\n                    <ul id=\"d-toc\">\n                          <div class=\"d-toc-close-wrapper mobile\">\n                            <div class=\"post-bottom-wrapper\">\n                              <a href=\"#bottom-anchor\" title=\"").concat(I18n.t(themePrefix("post_bottom_tooltip")), "\">").concat(iconHTML("downward"), "</a>\n                              </div>\n                            <div class=\"d-toc-close\">\n                              ").concat(closeIcon, "\n                            </div>\n                          </div>\n                        </ul>\n                  </div>\n              ")).parents(".topic-post").addClass("d-toc-post").parents("body").addClass("d-toc-timeline");
            $("#d-toc").dToc({
              cooked: body,
              selectors: dTocHeadingSelectors
            });
          });
        }, {
          id: "disco-toc"
        });
        api.cleanupStream(function () {
          $(window).off("scroll.d-toc");
          $("#main").off("click.toggleDtoc");
          $(".d-toc-timeline").removeClass("d-toc-timeline d-toc-timeline-visible");
        });
        api.onAppEvent("topic:current-post-changed", function (post) {
          if (!$(".d-toc-timeline").length) return;
          run.scheduleOnce("afterRender", function () {
            if (post.post.post_number <= 2) {
              $("body").removeClass("d-toc-timeline-visible");
              $(".d-toc-toggle").fadeIn(100);
            } else {
              $("body").addClass("d-toc-timeline-visible");
              $(".d-toc-toggle").fadeOut(100);
            }
          });
        });

        if (currUserTrustLevel >= minimumTrustLevel) {
          if (!I18n.translations[I18n.currentLocale()].js.composer) {
            I18n.translations[I18n.currentLocale()].js.composer = {};
          }

          I18n.translations[I18n.currentLocale()].js.composer.contains_dtoc = " ";
          api.addToolbarPopupMenuOptionsCallback(function () {
            var composerController = api.container.lookup("controller:composer");
            return {
              action: "insertDtoc",
              icon: "align-left",
              label: themePrefix("insert_table_of_contents"),
              condition: composerController.get("model.canCategorize")
            };
          });
          api.modifyClass("controller:composer", {
            pluginId: "DiscoTOC",
            actions: {
              insertDtoc: function insertDtoc() {
                this.get("toolbarEvent").applySurround("<div data-theme-toc=\"true\">", "</div>", "contains_dtoc");
              }
            }
          });
        }
      });
    }
  };
  _exports.default = _default;
});
}
