#!/bin/bash -e

JAVA_OPTS='--add-opens java.base/java.net=ALL-UNNAMED --add-opens java.base/java.lang.invoke=ALL-UNNAMED'

if [ ! -d /var/gerrit/git/All-Projects.git ]; then
  java $JAVA_OPTS -jar /var/gerrit/bin/gerrit.war init --batch --install-all-plugins -d /var/gerrit
  java $JAVA_OPTS -jar /var/gerrit/bin/gerrit.war reindex -d /var/gerrit
fi

exec java $JAVA_OPTS -jar /var/gerrit/bin/gerrit.war daemon -d /var/gerrit --console-log
